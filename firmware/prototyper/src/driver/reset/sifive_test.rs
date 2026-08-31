//! SiFive test finisher: board exit and reset signaling.

use crate::driver::reset::ResetDevice;
use crate::platform::mmio::Mmio;

/// Register-block span covering the single finish register.
pub(super) const SPAN: usize = 4;

/// Finish register values.
const FINISH_PASS: u32 = 0x5555;
const FINISH_RESET: u32 = 0x7777;
const FINISH_FAIL: u32 = 0x3333;

/// Register offsets within the test device register map.
#[derive(Clone, Copy)]
enum Reg {
    /// Finish (write-only).
    Finish,
}

impl Reg {
    /// Byte offset of this register.
    fn offset(self) -> usize {
        match self {
            Reg::Finish => 0,
        }
    }
}

/// SiFive test device used by QEMU to exit or reset.
pub(super) struct SifiveTestDevice {
    mmio: Mmio,
}

impl SifiveTestDevice {
    /// Wraps an acquired register block.
    pub(super) fn new(mmio: Mmio) -> Self {
        Self { mmio }
    }

    /// Writes the finish value and parks the hart until the board powers off.
    fn finish(&self, value: u32) -> ! {
        self.mmio.write::<u32>(Reg::Finish.offset(), value);
        loop {
            riscv::asm::wfi();
        }
    }
}

impl ResetDevice for SifiveTestDevice {
    #[inline]
    fn fail(&self, code: u16) -> ! {
        self.finish(FINISH_FAIL | (code as u32) << 16)
    }

    #[inline]
    fn pass(&self) -> ! {
        self.finish(FINISH_PASS)
    }

    #[inline]
    fn reset(&self) -> ! {
        self.finish(FINISH_RESET)
    }
}
