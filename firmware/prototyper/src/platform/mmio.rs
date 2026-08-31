//! MMIO region handles.

use core::mem::{align_of, size_of};

use super::BoardInfo;

/// Whether `[base, base + len)` fits inside one `[region_base, region_end)` range.
pub(crate) fn range_contains(
    region_base: usize,
    region_len: usize,
    base: usize,
    len: usize,
) -> bool {
    let Some(region_end) = region_base.checked_add(region_len) else {
        return false;
    };
    let Some(end) = base.checked_add(len) else {
        return false;
    };
    len != 0 && base >= region_base && end <= region_end
}

/// Resolves and validates the address of one register access.
pub(crate) fn checked_access_address(
    base: usize,
    len: usize,
    offset: usize,
    size: usize,
    align: usize,
) -> Option<usize> {
    let end = offset.checked_add(size)?;
    if end > len {
        return None;
    }
    let address = base.checked_add(offset)?;
    (address % align == 0).then_some(address)
}

/// A bounded handle to one MMIO register block.
///
/// Physical addresses are represented by `usize`. This is sufficient for the
/// hardware currently supported by Prototyper, whose MMIO addresses fit within
/// XLEN; a platform with a wider physical address space would need a multiword
/// address representation.
#[derive(Clone, Copy)]
pub(crate) struct Mmio {
    base: usize,
    len: usize,
}

impl Mmio {
    /// Creates a handle after the caller has established that the entire
    /// register block names accessible device memory.
    ///
    /// # Safety
    ///
    /// `[base, base + len)` must be a valid MMIO region for the lifetime of
    /// the firmware, and the drivers using it must select register widths
    /// implemented by that device.
    const unsafe fn new(base: usize, len: usize) -> Self {
        Self { base, len }
    }

    /// Acquires `[base, base + len)` from the board's trusted MMIO regions.
    pub(crate) fn within(board: &BoardInfo, base: usize, len: usize) -> Option<Self> {
        board
            .mmio_regions
            .iter()
            .any(|&(region_base, region_len)| range_contains(region_base, region_len, base, len))
            // SAFETY: `BoardInfo::mmio_regions` is private to the platform
            // discovery module. It contains only non-empty FDT `reg` windows
            // and machine-model-fixed windows recorded by that module.
            .then(|| unsafe { Self::new(base, len) })
    }

    fn access_address<T: MmioValue>(&self, offset: usize) -> usize {
        checked_access_address(self.base, self.len, offset, size_of::<T>(), align_of::<T>())
            .expect("invalid MMIO access bounds, arithmetic, or alignment")
    }

    /// Reads an integer register with volatile semantics.
    #[inline]
    pub(crate) fn read<T: MmioValue>(&self, offset: usize) -> T {
        let address = self.access_address::<T>(offset);
        // SAFETY: `within` confines the address to an FDT-discovered or
        // platform-defined device region; `access_address` checks bounds,
        // overflow, and alignment; and `MmioValue` accepts every bit pattern.
        // Each driver selects a register width implemented by that device and
        // the firmware accesses its regions only from M-mode.
        unsafe { (address as *const T).read_volatile() }
    }

    /// Writes an integer register with volatile semantics.
    #[inline]
    pub(crate) fn write<T: MmioValue>(&self, offset: usize, value: T) {
        let address = self.access_address::<T>(offset);
        // SAFETY: the address validity, bounds, alignment, accessibility,
        // and register-width invariants are the same as for `read`; `value`
        // is an MMIO integer.
        unsafe { (address as *mut T).write_volatile(value) }
    }
}

/// Integer types whose every bit pattern is valid for volatile MMIO reads.
///
/// # Safety
///
/// Implementations must be unsigned integers: every bit pattern has to be
/// a valid, initialized value for volatile loads from device memory.
pub(crate) unsafe trait MmioValue: Copy {}

unsafe impl MmioValue for u8 {}
unsafe impl MmioValue for u16 {}
unsafe impl MmioValue for u32 {}
unsafe impl MmioValue for u64 {}
