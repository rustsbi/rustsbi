//! SiFive Core Local Interruptor (CLINT).

use crate::driver::{IpiDevice, TimerDevice};
use crate::platform::mmio::Mmio;

/// Register-block span covering `mtime` at offset 0xbff8.
pub(super) const SPAN: usize = 0xc000;

/// Register offsets within the SiFive CLINT register map.
#[derive(Clone, Copy)]
enum Reg {
    /// Machine software interrupt pending (per hart).
    Msip,
    /// Machine timer compare (per hart).
    Mtimecmp,
    /// Machine time.
    Mtime,
}

impl Reg {
    /// Byte offset of this register; per-hart registers take the hart index.
    fn offset(self, hart: usize) -> usize {
        match self {
            Reg::Msip => hart * 4,
            Reg::Mtimecmp => 0x4000 + hart * 8,
            Reg::Mtime => 0xbff8,
        }
    }
}

pub(super) struct SifiveClint {
    mmio: Mmio,
}

impl SifiveClint {
    /// Wraps an acquired register block.
    pub(super) fn new(mmio: Mmio) -> Self {
        Self { mmio }
    }
}

impl TimerDevice for SifiveClint {
    #[inline(always)]
    fn read_time(&self) -> u64 {
        self.mmio.read::<u64>(Reg::Mtime.offset(0))
    }

    #[inline(always)]
    fn set_timer(&self, hart_idx: usize, val: u64) {
        self.mmio.write::<u64>(Reg::Mtimecmp.offset(hart_idx), val)
    }
}

impl IpiDevice for SifiveClint {
    #[inline(always)]
    fn send_ipi(&self, hart_idx: usize) {
        self.mmio.write::<u32>(Reg::Msip.offset(hart_idx), 1)
    }

    #[inline(always)]
    fn clear_ipi(&self) {
        self.mmio
            .write::<u32>(Reg::Msip.offset(crate::riscv::current_hartid()), 0)
    }
}
