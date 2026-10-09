//! Checked physical-memory protection plans for the current hart.
//!
//! Firmware policy supplies ordered regions and permissions.
//! Runtime validates the complete plan before changing hardware and owns
//! the CSR sequence.

#![deny(unsafe_code)]

mod csr;

use core::fmt;
use core::marker::PhantomData;

use crate::hart::{HartId, HartIdError};

const ADDRESS_MODE_SHIFT: u8 = 3;
const ADDRESS_MODE_MASK: u8 = 0b11 << ADDRESS_MODE_SHIFT;

/// Maximum number of entries supported by this Runtime implementation.
pub const MAX_ENTRIES: usize = 16;

/// Access permissions for a PMP region.
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Permission {
    /// No read, write, or execute access.
    None = 0,
    /// Read access.
    Read = 1,
    /// Write-only encoding, reserved by the base PMP architecture.
    Write = 2,
    /// Read and write access.
    ReadWrite = 3,
    /// Execute access.
    Execute = 4,
    /// Read and execute access.
    ReadExecute = 5,
    /// Write and execute encoding, reserved by the base PMP architecture.
    WriteExecute = 6,
    /// Read, write, and execute access.
    ReadWriteExecute = 7,
}

impl fmt::Display for Permission {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.pad(match self {
            Self::None => "NONE",
            Self::Read => "R",
            Self::Write => "W",
            Self::ReadWrite => "RW",
            Self::Execute => "X",
            Self::ReadExecute => "RX",
            Self::WriteExecute => "WX",
            Self::ReadWriteExecute => "RWX",
        })
    }
}

/// An address-matching mode read from the hardware configuration.
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AddressMode {
    /// Disabled entry, whose address may bound the following TOR entry.
    Off = 0,
    /// Top-of-range matching, bounded below by the preceding entry's address.
    Tor = 1,
    /// Naturally aligned four-byte region.
    Na4 = 2,
    /// Naturally aligned power-of-two region.
    Napot = 3,
}

impl fmt::Display for AddressMode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.pad(match self {
            Self::Off => "OFF",
            Self::Tor => "TOR",
            Self::Na4 => "NA4",
            Self::Napot => "NAPOT",
        })
    }
}

/// A physical-address region in an ordered PMP plan.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Region {
    /// Disabled entry supplying a byte-address boundary for the next entry.
    Off(u64),
    /// Top-of-range entry ending at an exclusive byte-address boundary.
    Tor(u64),
    /// Final TOR fallback ending at the largest native byte address supported
    /// by hardware, rounded down to a four-byte boundary.
    NativeAddressSpaceEnd,
    /// Naturally aligned power-of-two region, at least eight bytes wide.
    Napot {
        /// First physical byte address.
        base: u64,
        /// Region size in bytes.
        size: u64,
    },
    /// Final NAPOT fallback covering the full hardware physical address space.
    AllMemory,
}

/// One policy-selected entry in the complete PMP plan.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Entry {
    /// Address-matching policy.
    pub region: Region,
    /// Access permissions.
    pub permission: Permission,
    /// Locks the installed entry until this hart resets.
    pub locked: bool,
}

/// A typed, read-only snapshot of one PMP entry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Configuration {
    /// Address-matching mode.
    mode: AddressMode,
    /// Configured permissions.
    permission: Permission,
    /// Whether hardware has locked the entry.
    locked: bool,
    address: usize,
}

impl Configuration {
    /// Returns the address-matching mode.
    pub const fn mode(self) -> AddressMode {
        self.mode
    }

    /// Returns the configured permissions.
    pub const fn permission(self) -> Permission {
        self.permission
    }

    /// Returns whether hardware has locked the entry.
    pub const fn is_locked(self) -> bool {
        self.locked
    }

    /// Returns the address field in byte-address units for diagnostic logs.
    /// NAPOT matching also encodes its size in this field.
    pub const fn address_field(self) -> u128 {
        (self.address as u128) << 2
    }
}

/// Failure to validate or install a physical-memory protection plan.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    /// The plan is empty or exceeds Runtime's supported entry count.
    InvalidEntryCount,
    /// The operation is not running on the object's published hart.
    InvalidHart,
    /// An entry has an invalid range, alignment, ordering, or permission.
    InvalidEntry(usize),
    /// A physical address cannot be encoded at this hart's XLEN.
    AddressOverflow(usize),
    /// A locked entry prevents updating the requested plan.
    LockedEntry(usize),
    /// Hardware rejected an address field, including an unimplemented entry.
    AddressRejected {
        /// Position of the entry.
        index: usize,
        /// Address field Runtime wrote.
        expected: usize,
        /// Address field hardware returned.
        observed: usize,
    },
    /// Hardware rejected a configuration byte or an entry is unimplemented.
    ConfigurationRejected {
        /// Position of the entry.
        index: usize,
        /// Configuration byte Runtime wrote.
        expected: u8,
        /// Configuration byte hardware returned.
        observed: u8,
    },
    /// A guarded hardware access faulted.
    Access(crate::trap::Error),
}

impl From<crate::trap::Error> for Error {
    fn from(error: crate::trap::Error) -> Self {
        Self::Access(error)
    }
}

#[derive(Clone, Copy)]
struct EncodedEntry {
    address: usize,
    config: u8,
    saturate: bool,
}

/// The PMP table of the hart executing this call chain.
///
/// The object stays on its owning hart. Installation validates the complete
/// plan before entering the CSR commit and rollback transaction.
pub struct Pmp {
    hart: HartId,
    _hart_local: PhantomData<*mut ()>,
}

impl Pmp {
    /// Binds protection operations to the current published hart.
    pub fn current() -> Result<Self, HartIdError> {
        Ok(Self {
            hart: HartId::current()?,
            _hart_local: PhantomData,
        })
    }

    fn validate_hart(&self) -> Result<(), Error> {
        if HartId::current().map_err(|_| Error::InvalidHart)? != self.hart {
            return Err(Error::InvalidHart);
        }
        Ok(())
    }

    /// Installs the first entries of the current hart's protection table.
    ///
    /// The plan must use nondecreasing OFF/TOR boundaries.
    /// NAPOT regions follow earlier boundaries, and an all-memory fallback must be last.
    /// Equal TOR boundaries are allowed for empty linker sections.
    /// Unmentioned entries keep their configuration.
    /// Existing locks reject the operation before any write.
    ///
    /// Machine interrupts remain disabled while address fields and configuration bytes
    /// are installed and checked.
    /// Errors returned after hardware writes restore and verify the previous table
    /// before re-enabling interrupts.
    /// If restoration fails, or a hardware error occurs while committing locks,
    /// Runtime stops this hart without returning a partial plan.
    /// Installation and rollback synchronize cached translation and PMP permissions.
    pub fn install(&self, plan: &[Entry]) -> Result<(), Error> {
        let encoded = encode_plan(plan, usize::BITS)?;
        self.validate_hart()?;
        csr::install(&encoded[..plan.len()])
    }

    /// Reads a typed snapshot of the first `count` entries of the current hart.
    /// Returns a fixed-size array whose first `count` entries contain the snapshot.
    pub fn snapshot(&self, count: usize) -> Result<[Configuration; MAX_ENTRIES], Error> {
        if count == 0 || count > MAX_ENTRIES {
            return Err(Error::InvalidEntryCount);
        }
        self.validate_hart()?;
        csr::snapshot(count)
    }
}

fn encode_plan(plan: &[Entry], xlen: u32) -> Result<[EncodedEntry; MAX_ENTRIES], Error> {
    if plan.is_empty() || plan.len() > MAX_ENTRIES {
        return Err(Error::InvalidEntryCount);
    }
    let address_limit = if xlen == 32 {
        u32::MAX as u64
    } else {
        u64::MAX
    };
    let mut encoded = [EncodedEntry {
        address: 0,
        config: 0,
        saturate: false,
    }; MAX_ENTRIES];
    let mut previous_end = 0;
    for (index, entry) in plan.iter().enumerate() {
        if matches!(
            entry.permission,
            Permission::Write | Permission::WriteExecute
        ) {
            return Err(Error::InvalidEntry(index));
        }
        let (mode, address, saturate) = match entry.region {
            Region::Off(boundary) | Region::Tor(boundary) => {
                if boundary & 3 != 0 || boundary < previous_end {
                    return Err(Error::InvalidEntry(index));
                }
                previous_end = boundary;
                let mode = if matches!(entry.region, Region::Off(_)) {
                    if entry.permission != Permission::None {
                        return Err(Error::InvalidEntry(index));
                    }
                    AddressMode::Off
                } else {
                    AddressMode::Tor
                };
                (mode, boundary >> 2, false)
            }
            Region::Napot { base, size } => {
                if size < 8
                    || !size.is_power_of_two()
                    || base & (size - 1) != 0
                    || base < previous_end
                {
                    return Err(Error::InvalidEntry(index));
                }
                previous_end = base
                    .checked_add(size)
                    .ok_or(Error::AddressOverflow(index))?;
                (AddressMode::Napot, (base >> 2) | ((size >> 3) - 1), false)
            }
            Region::NativeAddressSpaceEnd => {
                if index + 1 != plan.len() || previous_end > address_limit & !3 {
                    return Err(Error::InvalidEntry(index));
                }
                (AddressMode::Tor, address_limit >> 2, true)
            }
            Region::AllMemory => {
                if index + 1 != plan.len() {
                    return Err(Error::InvalidEntry(index));
                }
                (AddressMode::Napot, address_limit, true)
            }
        };
        if address > address_limit {
            return Err(Error::AddressOverflow(index));
        }
        encoded[index] = EncodedEntry {
            address: address as usize,
            config: entry.permission as u8
                | (mode as u8) << ADDRESS_MODE_SHIFT
                | u8::from(entry.locked) << 7,
            saturate,
        };
    }
    Ok(encoded)
}

#[cfg(test)]
mod tests {
    use super::*;

    const fn entry(region: Region) -> Entry {
        Entry {
            region,
            permission: Permission::None,
            locked: false,
        }
    }

    #[test]
    fn rejects_bad_boundaries_before_hardware_access() {
        assert!(matches!(
            encode_plan(&[], 64),
            Err(Error::InvalidEntryCount)
        ));
        assert!(matches!(
            encode_plan(&[entry(Region::Off(0)); MAX_ENTRIES + 1], 64),
            Err(Error::InvalidEntryCount)
        ));
        assert!(matches!(
            encode_plan(&[entry(Region::Tor(3))], 64),
            Err(Error::InvalidEntry(0))
        ));
        assert!(matches!(
            encode_plan(&[entry(Region::Tor(8)), entry(Region::Off(4))], 64),
            Err(Error::InvalidEntry(1))
        ));
        assert!(encode_plan(&[entry(Region::Off(8)), entry(Region::Tor(8))], 64).is_ok());
        assert!(matches!(
            encode_plan(&[entry(Region::AllMemory), entry(Region::Tor(8))], 64),
            Err(Error::InvalidEntry(0))
        ));
    }

    #[test]
    fn rv32_accepts_wide_physical_addresses_only_when_encodable() {
        let max = (u32::MAX as u64) << 2;
        assert_eq!(
            encode_plan(&[entry(Region::Tor(max))], 32).unwrap()[0].address,
            u32::MAX as usize
        );
        assert!(matches!(
            encode_plan(&[entry(Region::Tor(max + 4))], 32),
            Err(Error::AddressOverflow(0))
        ));
        assert_eq!(
            encode_plan(&[entry(Region::AllMemory)], 32).unwrap()[0].address,
            u32::MAX as usize
        );
        assert!(matches!(
            encode_plan(
                &[
                    entry(Region::Tor(1u64 << 32)),
                    entry(Region::NativeAddressSpaceEnd)
                ],
                32
            ),
            Err(Error::InvalidEntry(1))
        ));
    }

    #[test]
    fn validates_napot_size_alignment_and_reserved_permissions() {
        assert!(matches!(
            encode_plan(&[entry(Region::Napot { base: 0, size: 4 })], 64),
            Err(Error::InvalidEntry(0))
        ));
        assert!(matches!(
            encode_plan(&[entry(Region::Napot { base: 4, size: 8 })], 64),
            Err(Error::InvalidEntry(0))
        ));
        assert_eq!(
            encode_plan(&[entry(Region::Napot { base: 16, size: 16 })], 64).unwrap()[0].address,
            5
        );
        assert!(matches!(
            encode_plan(
                &[entry(Region::Napot {
                    base: 1 << 63,
                    size: 1 << 63
                })],
                64
            ),
            Err(Error::AddressOverflow(0))
        ));
        let mut bad = entry(Region::Tor(8));
        bad.permission = Permission::Write;
        assert!(matches!(
            encode_plan(&[bad], 64),
            Err(Error::InvalidEntry(0))
        ));
    }
}
