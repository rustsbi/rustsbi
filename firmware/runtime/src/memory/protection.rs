//! Machine-mode physical memory protection for firmware-owned memory.
//!
//! Firmware policy selects the platform windows that must remain inaccessible
//! to lower privilege modes. Runtime derives the firmware image and read-only
//! data bounds from its linker contract, validates the complete layout, and
//! performs the PMP register accesses.

#![cfg_attr(
    not(any(target_arch = "riscv32", target_arch = "riscv64")),
    allow(dead_code)
)]

use alloc::vec::Vec;
use core::fmt;

use super::{PhysAddrRange, image, locate_firmware_image};
use crate::{Error, Result};

/// PMP address registers describe physical addresses in four-byte units.
const PMP_ALIGNMENT: usize = 4;
/// The highest supported layout uses all sixteen entries.
const MAX_PMP_ENTRIES: usize = 16;
/// One firmware layout needs at most three denied device windows.
const MAX_DENIED_WINDOWS: usize = 3;

/// A PMP address-matching mode.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PmpRange {
    /// The entry is disabled.
    Off,
    /// Top of range: the entry starts at the previous PMP address.
    Tor,
    /// Naturally aligned four-byte region.
    Na4,
    /// Naturally aligned power-of-two region.
    Napot,
}

impl fmt::Display for PmpRange {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.pad(match self {
            Self::Off => "OFF",
            Self::Tor => "TOR",
            Self::Na4 => "NA4",
            Self::Napot => "NAPOT",
        })
    }
}

/// A PMP permission encoding.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PmpPermission {
    /// No access.
    None,
    /// Read-only access.
    Read,
    /// Write-only access.
    Write,
    /// Read and write access.
    ReadWrite,
    /// Execute-only access.
    Execute,
    /// Read and execute access.
    ReadExecute,
    /// Write and execute access.
    WriteExecute,
    /// Read, write, and execute access.
    ReadWriteExecute,
}

impl fmt::Display for PmpPermission {
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

/// One PMP entry read from the current hart.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PmpEntry {
    index: usize,
    range: PmpRange,
    permission: PmpPermission,
    address: usize,
}

impl PmpEntry {
    const fn new(index: usize, range: PmpRange, permission: PmpPermission, address: usize) -> Self {
        Self {
            index,
            range,
            permission,
            address,
        }
    }

    /// Returns the zero-based PMP entry index.
    pub const fn index(self) -> usize {
        self.index
    }

    /// Returns the address-matching mode.
    pub const fn range(self) -> PmpRange {
        self.range
    }

    /// Returns the access permission.
    pub const fn permission(self) -> PmpPermission {
        self.permission
    }

    /// Returns the raw PMP address register value, which encodes bytes in
    /// four-byte units.
    pub const fn address(self) -> usize {
        self.address
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Entry {
    range: PmpRange,
    permission: PmpPermission,
    address: usize,
}

impl Entry {
    const fn new(range: PmpRange, permission: PmpPermission, address: usize) -> Self {
        Self {
            range,
            permission,
            address,
        }
    }
}

#[derive(Clone, Copy)]
struct ImageBounds {
    start: usize,
    end: usize,
    rodata_start: usize,
    rodata_end: usize,
}

/// A validated firmware PMP layout.
///
/// The layout grants firmware RAM read, write, and execute access, makes the
/// image bytes before rodata read-only, denies rodata and later image bytes,
/// denies each selected device window, and optionally denies the firmware's
/// non-cacheable alias.
/// Constructing a value validates ordering, bounds, and PMP address
/// alignment; hardware is not accessed until [`Self::install`] runs.
pub struct FirmwareProtection {
    entries: Vec<Entry>,
}

impl FirmwareProtection {
    /// Derives a firmware protection layout from the current firmware image.
    ///
    /// `firmware_ram` is the RAM bank containing the linked image. `denied`
    /// selects ordered, non-overlapping device windows that lower privilege
    /// modes must not access. `noncacheable_alias` is the optional physical
    /// alias offset applied to the firmware image.
    ///
    /// Returns [`Error::InvalidArgs`] for empty or unaligned windows, windows
    /// that reach into or past firmware RAM, out-of-order or overlapping
    /// windows, or a layout that does not fit the first sixteen PMP entries.
    pub fn new(
        firmware_ram: PhysAddrRange,
        denied: impl IntoIterator<Item = PhysAddrRange>,
        noncacheable_alias: Option<u64>,
    ) -> Result<Self> {
        let denied: Vec<_> = denied.into_iter().collect();
        let image = locate_firmware_image()?;
        let (rodata_start, rodata_end) = image::linker_rodata_bounds()?;
        let image = ImageBounds {
            start: image.start().as_usize(),
            end: image.end().as_usize(),
            rodata_start,
            rodata_end,
        };
        Self::from_bounds(firmware_ram, &denied, noncacheable_alias, image)
    }

    fn from_bounds(
        firmware_ram: PhysAddrRange,
        denied: &[PhysAddrRange],
        noncacheable_alias: Option<u64>,
        image: ImageBounds,
    ) -> Result<Self> {
        validate_denied_windows(firmware_ram, denied)?;
        Ok(Self {
            entries: build_entries(firmware_ram, denied, noncacheable_alias, image)?,
        })
    }

    /// Installs this layout in the current hart's PMP.
    ///
    /// Firmware calls this in machine mode on each hart before entering a
    /// lower privilege mode. Installing an already-validated layout changes
    /// only PMP permissions and address boundaries; it performs no memory
    /// access and does not lock any entry.
    pub fn install(&self) -> Result<()> {
        #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
        {
            for (index, entry) in self.entries.iter().copied().enumerate() {
                // SAFETY: Construction validated the entry order, bounds, and
                // alignment. The caller installs the layout on the current
                // hart in machine mode before lower privilege modes execute.
                unsafe {
                    write_config(index, entry)?;
                    write_address(index, entry.address)?;
                }
            }
            Ok(())
        }
        #[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
        {
            Err(Error::NotSupported)
        }
    }

    /// Reads the current hart's PMP entries for this layout.
    ///
    /// The returned entries report the hardware encoding, including any
    /// platform-defined read-only bits.
    pub fn read(&self) -> Result<Vec<PmpEntry>> {
        #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
        {
            self.entries
                .iter()
                .enumerate()
                .map(|(index, _)| read_entry(index))
                .collect()
        }
        #[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
        {
            Err(Error::NotSupported)
        }
    }
}

fn validate_denied_windows(firmware_ram: PhysAddrRange, denied: &[PhysAddrRange]) -> Result<()> {
    if denied.len() > MAX_DENIED_WINDOWS {
        return Err(Error::InvalidArgs);
    }
    if !firmware_ram.start().is_aligned_to(PMP_ALIGNMENT)
        || !firmware_ram.end().is_aligned_to(PMP_ALIGNMENT)
    {
        return Err(Error::InvalidArgs);
    }

    let mut previous_end = None;
    for window in denied.iter().copied() {
        if !window.start().is_aligned_to(PMP_ALIGNMENT)
            || !window.end().is_aligned_to(PMP_ALIGNMENT)
        {
            return Err(Error::InvalidArgs);
        }
        // Denied windows precede firmware RAM in the TOR sequence. A window
        // that reaches into or past RAM would make the next TOR boundary move
        // backwards, so reject it rather than program a silently empty range.
        if window.end() > firmware_ram.start() {
            return Err(Error::InvalidArgs);
        }
        if previous_end.is_some_and(|end| end > window.start()) {
            return Err(Error::InvalidArgs);
        }
        previous_end = Some(window.end());
    }
    Ok(())
}

fn build_entries(
    firmware_ram: PhysAddrRange,
    denied: &[PhysAddrRange],
    noncacheable_alias: Option<u64>,
    image: ImageBounds,
) -> Result<Vec<Entry>> {
    for value in [image.start, image.end, image.rodata_start, image.rodata_end] {
        if value & (PMP_ALIGNMENT - 1) != 0 {
            return Err(Error::InvalidArgs);
        }
    }
    if firmware_ram.start().as_usize() > image.start
        || image.start > image.rodata_start
        || image.rodata_start > image.rodata_end
        || image.rodata_end > image.end
        || image.end > firmware_ram.end().as_usize()
    {
        return Err(Error::InvalidArgs);
    }

    let mut entries = Vec::with_capacity(MAX_PMP_ENTRIES);
    entries.push(Entry::new(PmpRange::Off, PmpPermission::None, 0));
    for window in denied.iter().copied() {
        entries.push(Entry::new(
            PmpRange::Tor,
            PmpPermission::ReadWriteExecute,
            window.start().as_usize() >> 2,
        ));
        entries.push(Entry::new(
            PmpRange::Tor,
            PmpPermission::None,
            window.end().as_usize() >> 2,
        ));
    }

    // Firmware RAM is writable. This read-only TOR entry ends at rodata_start,
    // so the image bytes before rodata are read-only, while rodata itself and
    // all later image bytes are inaccessible.
    entries.push(Entry::new(
        PmpRange::Tor,
        PmpPermission::ReadWriteExecute,
        firmware_ram.start().as_usize() >> 2,
    ));
    entries.push(Entry::new(
        PmpRange::Tor,
        PmpPermission::ReadWriteExecute,
        image.start >> 2,
    ));
    entries.push(Entry::new(
        PmpRange::Tor,
        PmpPermission::Read,
        image.rodata_start >> 2,
    ));
    entries.push(Entry::new(
        PmpRange::Tor,
        PmpPermission::None,
        image.rodata_end >> 2,
    ));
    entries.push(Entry::new(
        PmpRange::Tor,
        PmpPermission::None,
        image.end >> 2,
    ));
    entries.push(Entry::new(
        PmpRange::Tor,
        PmpPermission::ReadWriteExecute,
        firmware_ram.end().as_usize() >> 2,
    ));

    if let Some(alias) = noncacheable_alias {
        if alias == 0 || !alias.is_power_of_two() || alias < firmware_ram.end().as_usize() as u64 {
            return Err(Error::InvalidArgs);
        }
        let start = (image.start as u64)
            .checked_add(alias)
            .ok_or(Error::Overflow)?;
        let end = (image.end as u64)
            .checked_add(alias)
            .ok_or(Error::Overflow)?;
        if start & (PMP_ALIGNMENT as u64 - 1) != 0 || end & (PMP_ALIGNMENT as u64 - 1) != 0 {
            return Err(Error::InvalidArgs);
        }
        let start = start >> 2;
        let end = end >> 2;
        if start > usize::MAX as u64 || end > usize::MAX as u64 {
            return Err(Error::InvalidArgs);
        }
        // Deny the alias, then restore the wider physical address space.
        entries.push(Entry::new(
            PmpRange::Off,
            PmpPermission::None,
            start as usize,
        ));
        entries.push(Entry::new(PmpRange::Tor, PmpPermission::None, end as usize));
        entries.push(Entry::new(
            PmpRange::Napot,
            PmpPermission::ReadWriteExecute,
            usize::MAX,
        ));
    } else {
        // Permit the remaining physical address space.
        entries.push(Entry::new(
            PmpRange::Tor,
            PmpPermission::ReadWriteExecute,
            usize::MAX >> 2,
        ));
    }

    if entries.len() > MAX_PMP_ENTRIES {
        return Err(Error::InvalidArgs);
    }
    Ok(entries)
}

#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
unsafe fn write_config(index: usize, entry: Entry) -> Result<()> {
    use core::mem::size_of;
    use riscv::register::{self, Permission, Range};

    let range = match entry.range {
        PmpRange::Off => Range::OFF,
        PmpRange::Tor => Range::TOR,
        PmpRange::Na4 => Range::NA4,
        PmpRange::Napot => Range::NAPOT,
    };
    let permission = match entry.permission {
        PmpPermission::None => Permission::NONE,
        PmpPermission::Read => Permission::R,
        PmpPermission::Write => Permission::W,
        PmpPermission::ReadWrite => Permission::RW,
        PmpPermission::Execute => Permission::X,
        PmpPermission::ReadExecute => Permission::RX,
        PmpPermission::WriteExecute => Permission::WX,
        PmpPermission::ReadWriteExecute => Permission::RWX,
    };
    let slot = index % size_of::<usize>();
    // SAFETY: The caller installs a validated layout on the current hart.
    // The match selects the configuration register that owns `slot` for this
    // XLEN, and construction limited `index` to the first sixteen entries.
    unsafe {
        match index / size_of::<usize>() {
            0 => register::pmpcfg0::set_pmp(slot, range, permission, false),
            #[cfg(target_pointer_width = "32")]
            1 => register::pmpcfg1::set_pmp(slot, range, permission, false),
            #[cfg(target_pointer_width = "32")]
            2 => register::pmpcfg2::set_pmp(slot, range, permission, false),
            #[cfg(target_pointer_width = "32")]
            3 => register::pmpcfg3::set_pmp(slot, range, permission, false),
            #[cfg(target_pointer_width = "64")]
            1 => register::pmpcfg2::set_pmp(slot, range, permission, false),
            _ => return Err(Error::InvalidArgs),
        }
    }
    Ok(())
}

#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
unsafe fn write_address(index: usize, address: usize) -> Result<()> {
    use riscv::register::*;
    // SAFETY: The caller installs a validated layout on the current hart.
    // Construction limited `index` to the first sixteen PMP address registers.
    unsafe {
        match index {
            0 => pmpaddr0::write(address),
            1 => pmpaddr1::write(address),
            2 => pmpaddr2::write(address),
            3 => pmpaddr3::write(address),
            4 => pmpaddr4::write(address),
            5 => pmpaddr5::write(address),
            6 => pmpaddr6::write(address),
            7 => pmpaddr7::write(address),
            8 => pmpaddr8::write(address),
            9 => pmpaddr9::write(address),
            10 => pmpaddr10::write(address),
            11 => pmpaddr11::write(address),
            12 => pmpaddr12::write(address),
            13 => pmpaddr13::write(address),
            14 => pmpaddr14::write(address),
            15 => pmpaddr15::write(address),
            _ => return Err(Error::InvalidArgs),
        }
    }
    Ok(())
}

#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
fn read_entry(index: usize) -> Result<PmpEntry> {
    use core::mem::size_of;
    use riscv::register::*;

    let slot = index % size_of::<usize>();
    let config = match index / size_of::<usize>() {
        0 => pmpcfg0::read().into_config(slot),
        #[cfg(target_pointer_width = "32")]
        1 => pmpcfg1::read().into_config(slot),
        #[cfg(target_pointer_width = "32")]
        2 => pmpcfg2::read().into_config(slot),
        #[cfg(target_pointer_width = "32")]
        3 => pmpcfg3::read().into_config(slot),
        #[cfg(target_pointer_width = "64")]
        1 => pmpcfg2::read().into_config(slot),
        _ => return Err(Error::InvalidArgs),
    };
    let range = match config.range {
        Range::OFF => PmpRange::Off,
        Range::TOR => PmpRange::Tor,
        Range::NA4 => PmpRange::Na4,
        Range::NAPOT => PmpRange::Napot,
    };
    let permission = match config.permission {
        Permission::NONE => PmpPermission::None,
        Permission::R => PmpPermission::Read,
        Permission::W => PmpPermission::Write,
        Permission::RW => PmpPermission::ReadWrite,
        Permission::X => PmpPermission::Execute,
        Permission::RX => PmpPermission::ReadExecute,
        Permission::WX => PmpPermission::WriteExecute,
        Permission::RWX => PmpPermission::ReadWriteExecute,
    };
    let address = match index {
        0 => pmpaddr0::read(),
        1 => pmpaddr1::read(),
        2 => pmpaddr2::read(),
        3 => pmpaddr3::read(),
        4 => pmpaddr4::read(),
        5 => pmpaddr5::read(),
        6 => pmpaddr6::read(),
        7 => pmpaddr7::read(),
        8 => pmpaddr8::read(),
        9 => pmpaddr9::read(),
        10 => pmpaddr10::read(),
        11 => pmpaddr11::read(),
        12 => pmpaddr12::read(),
        13 => pmpaddr13::read(),
        14 => pmpaddr14::read(),
        15 => pmpaddr15::read(),
        _ => return Err(Error::InvalidArgs),
    };
    Ok(PmpEntry::new(index, range, permission, address))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Error;
    use crate::memory::PhysAddr;

    fn range(start: usize, end: usize) -> PhysAddrRange {
        PhysAddrRange::new(PhysAddr::new(start), PhysAddr::new(end)).unwrap()
    }

    fn image(start: usize, rodata_start: usize, rodata_end: usize, end: usize) -> ImageBounds {
        ImageBounds {
            start,
            rodata_start,
            rodata_end,
            end,
        }
    }

    #[test]
    fn aia_windows_and_firmware_boundaries_match_prior_layout() {
        let firmware_ram = range(0x8000_0000, 0x8020_0000);
        let denied = [
            range(0x0200_0000, 0x0201_0000),
            range(0x0c00_0000, 0x0c00_4000),
            range(0x2800_0000, 0x2800_2000),
        ];
        let image = image(0x8001_0000, 0x8002_0000, 0x8003_0000, 0x8004_0000);
        let protection =
            FirmwareProtection::from_bounds(firmware_ram, &denied, None, image).unwrap();
        let expected = [
            Entry::new(PmpRange::Off, PmpPermission::None, 0),
            Entry::new(
                PmpRange::Tor,
                PmpPermission::ReadWriteExecute,
                0x0200_0000 >> 2,
            ),
            Entry::new(PmpRange::Tor, PmpPermission::None, 0x0201_0000 >> 2),
            Entry::new(
                PmpRange::Tor,
                PmpPermission::ReadWriteExecute,
                0x0c00_0000 >> 2,
            ),
            Entry::new(PmpRange::Tor, PmpPermission::None, 0x0c00_4000 >> 2),
            Entry::new(
                PmpRange::Tor,
                PmpPermission::ReadWriteExecute,
                0x2800_0000 >> 2,
            ),
            Entry::new(PmpRange::Tor, PmpPermission::None, 0x2800_2000 >> 2),
            Entry::new(
                PmpRange::Tor,
                PmpPermission::ReadWriteExecute,
                0x8000_0000 >> 2,
            ),
            Entry::new(
                PmpRange::Tor,
                PmpPermission::ReadWriteExecute,
                0x8001_0000 >> 2,
            ),
            Entry::new(PmpRange::Tor, PmpPermission::Read, 0x8002_0000 >> 2),
            Entry::new(PmpRange::Tor, PmpPermission::None, 0x8003_0000 >> 2),
            Entry::new(PmpRange::Tor, PmpPermission::None, 0x8004_0000 >> 2),
            Entry::new(
                PmpRange::Tor,
                PmpPermission::ReadWriteExecute,
                0x8020_0000 >> 2,
            ),
            Entry::new(
                PmpRange::Tor,
                PmpPermission::ReadWriteExecute,
                usize::MAX >> 2,
            ),
        ];
        assert_eq!(protection.entries, expected);
    }

    #[test]
    fn denied_windows_must_precede_ram_without_overlap() {
        let firmware_ram = range(0x8000_0000, 0x8020_0000);
        let image = image(0x8001_0000, 0x8002_0000, 0x8003_0000, 0x8004_0000);

        let after_ram = [range(0x9000_0000, 0x9000_1000)];
        assert_eq!(
            FirmwareProtection::from_bounds(firmware_ram, &after_ram, None, image).err(),
            Some(Error::InvalidArgs)
        );

        let out_of_order = [
            range(0x0c00_0000, 0x0c00_4000),
            range(0x0200_0000, 0x0201_0000),
        ];
        assert_eq!(
            FirmwareProtection::from_bounds(firmware_ram, &out_of_order, None, image).err(),
            Some(Error::InvalidArgs)
        );

        let overlap = [
            range(0x0200_0000, 0x0200_2000),
            range(0x0200_1000, 0x0200_3000),
        ];
        assert_eq!(
            FirmwareProtection::from_bounds(firmware_ram, &overlap, None, image).err(),
            Some(Error::InvalidArgs)
        );

        let reaches_ram = [range(0x7fff_f000, 0x8000_1000)];
        assert_eq!(
            FirmwareProtection::from_bounds(firmware_ram, &reaches_ram, None, image).err(),
            Some(Error::InvalidArgs)
        );
    }

    #[test]
    fn alias_layout_uses_rv32_encodable_shifted_bounds() {
        let firmware_ram = range(0x4000_0000, 0x4800_0000);
        let image = image(0x4000_0000, 0x4001_0000, 0x4002_0000, 0x4003_0000);
        let alias = 0x1_0000_0000u64;
        let protection =
            FirmwareProtection::from_bounds(firmware_ram, &[], Some(alias), image).unwrap();

        let alias_start = (image.start as u64 + alias) >> 2;
        let alias_end = (image.end as u64 + alias) >> 2;
        assert!(alias_start <= u32::MAX as u64);
        assert!(alias_end <= u32::MAX as u64);
        assert_eq!(protection.entries.len(), 10);
        assert_eq!(
            protection.entries[7],
            Entry::new(PmpRange::Off, PmpPermission::None, alias_start as usize)
        );
        assert_eq!(
            protection.entries[8],
            Entry::new(PmpRange::Tor, PmpPermission::None, alias_end as usize)
        );
        assert_eq!(
            protection.entries[9],
            Entry::new(PmpRange::Napot, PmpPermission::ReadWriteExecute, usize::MAX)
        );

        let below_ram_end = 0x4000_0000u64;
        assert_eq!(
            FirmwareProtection::from_bounds(firmware_ram, &[], Some(below_ram_end), image).err(),
            Some(Error::InvalidArgs)
        );
    }
}
