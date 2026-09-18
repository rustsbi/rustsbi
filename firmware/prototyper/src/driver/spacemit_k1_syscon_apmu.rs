//! SpacemiT K1 syscon APMU (Application PMU) peripheral driver.
//!
//! K1 protects parts of its APMU register block from S-mode; an access from
//! S-mode raises an access fault that M-mode emulates. This driver owns the
//! whole device-tree-described APMU window and serves both the access
//! dispatcher and the K1 hart-wake sequence.
//!
//! Compatible: "spacemit,k1-syscon-apmu"

#![forbid(unsafe_code)]

use runtime::memory::{DeviceRegisterRange, MemoryRegistry, MmioRegion};
use spin::Once;

/// Devicetree compatible string for this peripheral.
pub(crate) const COMPATIBLE: &str = "spacemit,k1-syscon-apmu";

/// PMU_CORE_STATUS, relative to the APMU base.
///
/// `K1_CORE_STATUS` (0xd428_2890) less the APMU base (0xd428_2800) in
/// `runtime::spacemit_k1`.
const CORE_STATUS_OFFSET: usize = 0x090;

/// The two per-cluster wakeup-control bases, relative to the APMU base.
///
/// `K1_WAKEUP_BASES` (0xd428_292c and 0xd428_2b24) less the APMU base in
/// `runtime::spacemit_k1`.
const WAKEUP_OFFSETS: [usize; 2] = [0x12c, 0x324];

/// Harts per K1 cluster; each cluster has its own wakeup-control base.
const HARTS_PER_CLUSTER: usize = 4;

/// The highest wake register end offset: cluster-1 base + 4 entries * 4 bytes.
const WAKE_MAX_END: usize = WAKEUP_OFFSETS[1] + HARTS_PER_CLUSTER * size_of::<u32>();

/// APMU ranges reachable only from M-mode, as (offset, length) from the APMU
/// base. An access is declined only when it lies completely inside one range.
///
/// Source: SpacemiT vendor OpenSBI `m_only_ranges[]` in
/// `platform/generic/spacemit/spacemit_k1.c` at commit fc02b891
/// (spacemit-com/opensbi). Reset-vector ranges (C0_RVBADDR and C1_RVBADDR)
/// lie outside 0xd4282800..0xd4282c00 and are not included here.
const M_ONLY_OFFSETS: &[(usize, usize)] = &[
    (0x0e4, 0x04), // PMU_C0_CAPMP_IDLE_CFG1      (0xd42828e4)
    (0x120, 0x0c), // PMU_C0_CAPMP_IDLE_CFG0 x3   (0xd4282920)
    (0x150, 0x08), // PMU_C0_CAPMP_IDLE_CFG2 x2   (0xd4282950)
    (0x160, 0x08), // PMU_CAP_CORE2_IDLE_CFG x2   (0xd4282960)
    (0x304, 0x20), // PMU_CAP_CORE4_IDLE_CFG x8   (0xd4282b04)
];

/// The APMU peripheral, owning the whole device-tree-described window.
pub(crate) struct SpacemitK1SysconApmu {
    base: usize,
    size: usize,
    mmio: MmioRegion,
}

impl SpacemitK1SysconApmu {
    /// Acquires the device-tree-described APMU window.
    ///
    /// Returns `Err` when the window is too small to cover all hart-wake
    /// registers or when `acquire_mmio` fails.
    pub(crate) fn bind(
        registers: DeviceRegisterRange,
        memory: &mut MemoryRegistry,
    ) -> runtime::Result<Self> {
        let base = registers.start().as_usize();
        let size = registers.end().as_usize() - base;
        if size < WAKE_MAX_END {
            return Err(runtime::Error::InvalidArgs);
        }
        let mmio = memory.acquire_mmio(registers)?;
        Ok(Self { base, size, mmio })
    }

    /// Reads PMU_CORE_STATUS.
    pub(crate) fn core_status(&self) -> runtime::Result<u32> {
        self.mmio.read::<u32>(CORE_STATUS_OFFSET)
    }

    /// Writes `request` to the wakeup register owned by `caller`.
    pub(crate) fn request_wakeup(&self, caller: usize, request: u32) -> runtime::Result<()> {
        let base = WAKEUP_OFFSETS
            .get(caller / HARTS_PER_CLUSTER)
            .ok_or(runtime::Error::InvalidArgs)?;
        let offset = base + (caller % HARTS_PER_CLUSTER) * size_of::<u32>();
        self.mmio.write::<u32>(offset, request)
    }

    /// Emulates an S-mode load of `width` bytes from physical `addr`.
    pub(crate) fn load(&self, addr: usize, width: usize) -> Option<usize> {
        let offset = check_access(self.base, self.size, addr, width)?;
        match width {
            1 => self
                .mmio
                .read::<u8>(offset)
                .ok()
                .map(|value| value as usize),
            2 => self
                .mmio
                .read::<u16>(offset)
                .ok()
                .map(|value| value as usize),
            4 => self
                .mmio
                .read::<u32>(offset)
                .ok()
                .map(|value| value as usize),
            #[cfg(target_pointer_width = "64")]
            8 => self
                .mmio
                .read::<u64>(offset)
                .ok()
                .map(|value| value as usize),
            _ => None,
        }
    }

    /// Emulates an S-mode store of `value` (`width` bytes) to physical `addr`.
    pub(crate) fn store(&self, addr: usize, width: usize, value: usize) -> bool {
        let Some(offset) = check_access(self.base, self.size, addr, width) else {
            return false;
        };
        match width {
            1 => self.mmio.write::<u8>(offset, value as u8).is_ok(),
            2 => self.mmio.write::<u16>(offset, value as u16).is_ok(),
            4 => self.mmio.write::<u32>(offset, value as u32).is_ok(),
            #[cfg(target_pointer_width = "64")]
            8 => self.mmio.write::<u64>(offset, value as u64).is_ok(),
            _ => false,
        }
    }
}

/// Range and M-only policy check for one emulated access.
///
/// Returns the byte offset from the APMU base when the complete access fits
/// inside the window at a supported width and is not M-only. Following the
/// vendor policy, an access counts as M-only only when it lies completely
/// inside one M-only range.
fn check_access(base: usize, size: usize, addr: usize, width: usize) -> Option<usize> {
    if !matches!(width, 1 | 2 | 4 | 8) {
        return None;
    }
    let offset = addr.checked_sub(base)?;
    let end = offset.checked_add(width)?;
    if end > size {
        return None;
    }
    for &(m_offset, m_len) in M_ONLY_OFFSETS {
        let m_end = m_offset.checked_add(m_len)?;
        if offset >= m_offset && end <= m_end {
            return None;
        }
    }
    Some(offset)
}

static APMU: Once<SpacemitK1SysconApmu> = Once::new();

/// Publishes the APMU device once during boot.
pub(crate) fn install(device: SpacemitK1SysconApmu) -> &'static SpacemitK1SysconApmu {
    APMU.call_once(|| device)
}

/// Returns the published APMU device.
pub(crate) fn get() -> Option<&'static SpacemitK1SysconApmu> {
    APMU.get()
}
