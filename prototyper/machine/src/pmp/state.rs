//! Semantic PMP configuration and private encoding values.

use alloc::vec::Vec;
use core::ops::Range;

pub(super) const MAX_PMP_ENTRIES: usize = 64;

bitflags::bitflags! {
    /// Lower-privilege access granted to one physical-memory interval.
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct Permissions: u8 {
        /// Permit loads.
        const READ = 1 << 0;
        /// Permit stores.
        const WRITE = 1 << 1;
        /// Permit instruction fetches.
        const EXECUTE = 1 << 2;
    }
}

/// One immutable lower-privilege physical-memory policy.
///
/// The firmware image and every machine-owned MMIO interval are always denied
/// independently of these grants. Addresses not covered by a grant are denied
/// by the RISC-V PMP default when PMP is present.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Configuration {
    pub(super) grants: Vec<Grant>,
}

impl Configuration {
    /// Creates an empty, deny-all lower-privilege policy.
    #[doc(hidden)]
    pub fn empty() -> Self {
        Self { grants: Vec::new() }
    }

    /// Adds one exact lower-privilege grant.
    #[doc(hidden)]
    pub fn grant(&mut self, range: Range<usize>, permissions: Permissions) -> Result<(), PmpError> {
        let region = Region::new(range.start, range.end)?;
        if permissions.is_empty()
            || permissions.contains(Permissions::WRITE) && !permissions.contains(Permissions::READ)
            || self
                .grants
                .iter()
                .any(|known| overlaps(known.region, region))
        {
            return Err(PmpError::InvalidRegion);
        }
        self.grants.push(Grant {
            region,
            permissions,
        });
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct Grant {
    pub(super) region: Region,
    pub(super) permissions: Permissions,
}

const fn overlaps(left: Region, right: Region) -> bool {
    left.start < right.end && right.start < left.end
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub(super) enum AddressMode {
    NaturallyAlignedFourBytes = 2 << 3,
    NaturallyAlignedPowerOfTwo = 3 << 3,
}

impl AddressMode {
    pub(super) const fn bits(self) -> u8 {
        self as u8
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct Region {
    pub(super) start: usize,
    pub(super) end: usize,
}

impl Region {
    pub(super) fn new(start: usize, end: usize) -> Result<Self, PmpError> {
        if start >= end {
            return Err(PmpError::InvalidRegion);
        }
        Ok(Self { start, end })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct Capability {
    pub(super) entries: usize,
    pub(super) granularity: usize,
    pub(super) napot_address_mask: usize,
}

impl Capability {
    pub(super) fn new(
        entries: usize,
        granularity: usize,
        napot_address_mask: usize,
    ) -> Result<Self, PmpError> {
        let address_mask_is_contiguous = napot_address_mask == usize::MAX
            || napot_address_mask
                .checked_add(1)
                .is_some_and(usize::is_power_of_two);
        if entries > MAX_PMP_ENTRIES
            || granularity < 4
            || !granularity.is_power_of_two()
            || (entries == 0 && napot_address_mask != 0)
            || (entries != 0 && (!address_mask_is_contiguous || napot_address_mask == 0))
        {
            return Err(PmpError::InvalidCapability);
        }
        Ok(Self {
            entries,
            granularity,
            napot_address_mask,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct Entry {
    pub(super) address: usize,
    pub(super) permissions: Permissions,
    pub(super) mode: AddressMode,
}

impl Entry {
    pub(super) const fn config_byte(self) -> u8 {
        self.permissions.bits() | self.mode.bits()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum Image {
    Protected {
        entries: Vec<Entry>,
        deny_count: usize,
    },
    TrustedWithoutPmp,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Failure while validating, compiling, or installing a PMP policy.
pub enum PmpError {
    /// A range is empty, misaligned, overlaps another grant, or has invalid rights.
    InvalidRegion,
    /// Probed PMP capability facts are internally inconsistent.
    InvalidCapability,
    /// A range cannot be represented by the probed `pmpaddr` width.
    AddressOutOfRange,
    /// Exact NAPOT/NA4 encoding would require enlarging a range.
    Unrepresentable,
    /// The exact policy needs more entries than the hart implements.
    InsufficientEntries,
    /// No PMP exists and the selected target is not explicitly trusted.
    PmpRequired,
    /// Existing locked PMP entries prevent safe replacement.
    LockedState,
    /// Security-extension state changes standard PMP semantics.
    ExtendedState,
    /// A required PMP CSR cannot be accessed.
    HardwareUnavailable,
    /// A PMP CSR access trapped for an unexpected reason.
    UnexpectedFault,
    /// Per-entry probe results disagree or policy was already published.
    InconsistentCapability,
    /// A WARL write did not read back as the exact compiled policy.
    VerificationFailed,
}
