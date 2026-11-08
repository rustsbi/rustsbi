//! T-Head C900-compatible Core Local Interruptor (CLINT).
//!
//! # References
//!
//! - Specification: [RISC-V ACLINT 1.0-rc4](https://github.com/riscvarchive/riscv-aclint/blob/4e570bfd3201f2c09e5afd290b5091526b0f099a/riscv-aclint.adoc) —
//!   “Backward Compatibility With SiFive CLINT” and its offset table.
//! - Devicetree binding: [SiFive CLINT](https://github.com/torvalds/linux/blob/a500db7819c50db59e55f1b4fa1c3baa5a2616f3/Documentation/devicetree/bindings/timer/sifive%2Cclint.yaml) —
//!   T-Head compatibles and the absence of a memory-mapped `mtime` register.
//! - Reference implementation: [OpenSBI MTIMER FDT driver](https://github.com/riscv-software-src/opensbi/blob/35511bc6ee1c9c17b6a89b44c52e2044bb51b979/lib/utils/timer/fdt_timer_mtimer.c) —
//!   the `thead,c900-clint` no-`mtime`, 32-bit-access quirks.
//! - Reference implementation: [OpenSBI MTIMER accessors](https://github.com/riscv-software-src/opensbi/blob/35511bc6ee1c9c17b6a89b44c52e2044bb51b979/lib/utils/timer/aclint_mtimer.c) —
//!   compare-safe split writes.
//!
//! T-Head exposes the 64-bit `mtimecmp` register as two 32-bit MMIO words;
//! [`TimerDevice::set_compare`] prevents a transient early timer interrupt.

use core::mem::{align_of, size_of};
use runtime::{
    Error, Result,
    hart::HartId,
    memory::{MemoryRegistry, MmioRegion},
    timer::TimerDevice,
};

use super::description::Desc;

// The ACLINT legacy mapping places MTIMECMP at offset 0x4000.
const MTIMECMP_OFFSET: usize = 0x4000;

/// T-Head CLINT timer-register offsets.
#[repr(usize)]
#[derive(Clone, Copy)]
enum TimerRegister {
    MtimecmpLow = 0,
    MtimecmpHigh = size_of::<u32>(),
}

impl TimerRegister {
    const fn offset(self) -> usize {
        self as usize
    }

    fn offset_for_hart(self, hart: HartId) -> usize {
        // Initialization checked the window through the highest enabled hart.
        hart.as_usize() * size_of::<u64>() + self.offset()
    }
}

/// A T-Head CLINT timer that accesses `mtimecmp` as 32-bit words.
pub(super) struct THeadTimer {
    mtimecmp: MmioRegion,
}

impl THeadTimer {
    /// Acquires the `mtimecmp` window through the highest enabled hart ID.
    pub(super) fn init(description: Desc, memory: &mut MemoryRegistry) -> Result<Self> {
        let size = HartId::upper_bound()?
            .checked_mul(size_of::<u64>())
            .ok_or(Error::Overflow)?;
        let registers = description.registers.subrange(MTIMECMP_OFFSET, size)?;
        if !registers.has_aligned_bounds(align_of::<u32>()) {
            return Err(Error::InvalidArgs);
        }
        Ok(Self {
            mtimecmp: memory.acquire(registers)?,
        })
    }

    fn write(&self, reg: TimerRegister, hart: HartId, value: u32) -> Result<()> {
        self.mtimecmp.write(reg.offset_for_hart(hart), value)
    }
}

impl TimerDevice for THeadTimer {
    #[inline(always)]
    fn set_compare(&self, hart: HartId, value: u64) -> Result<()> {
        let low = value as u32;
        let high = (value >> u32::BITS) as u32;

        // Avoid a transient earlier deadline while replacing the two halves.
        // Raise the temporary compare value first, then install the final high
        // and low words in the order used by OpenSBI's 32-bit MTIMER accessor.
        self.write(TimerRegister::MtimecmpLow, hart, u32::MAX)?;
        self.write(TimerRegister::MtimecmpHigh, hart, high)?;
        self.write(TimerRegister::MtimecmpLow, hart, low)
    }
}
