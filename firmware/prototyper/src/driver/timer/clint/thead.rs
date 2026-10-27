//! T-Head C900-compatible Core Local Interruptor timer.
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

use alloc::boxed::Box;
use core::mem::{align_of, size_of};
use runtime::{
    Error, Result,
    hart::HartId,
    memory::{DeviceRegisterRange, MemoryRegistry, MmioRegion},
    timer::TimerDevice,
};

use crate::devicetree::EnabledNode;
use crate::driver::clint;
use crate::driver::timer::{Description, Driver, Timer, hart_id_upper_bound};

/// Timer name reported in boot logs.
const NAME: &str = "THeadClint";

/// T-Head-layout timer driver.
pub(crate) struct THeadTimerDriver;

pub(crate) static THEAD: THeadTimerDriver = THeadTimerDriver;

impl Driver<Timer> for THeadTimerDriver {
    fn compatibles(&self) -> &'static [&'static str] {
        &clint::THEAD_COMPATIBLES
    }

    fn parse<'a, 'tree>(
        &self,
        platform: &'a runtime::PlatformView<'tree>,
        device: EnabledNode<'a, 'tree>,
        _compatible: &str,
    ) -> Result<Option<Description>> {
        let registers = platform
            .device_register(device.node())?
            .ok_or(Error::InvalidArgs)?;
        let hart_id_upper_bound = hart_id_upper_bound()?;
        Ok(Some(Description::new(NAME, registers, move |memory| {
            bind(registers, memory, hart_id_upper_bound)
        })))
    }
}

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

    fn offset_for_hart(self, hart_id: usize) -> usize {
        hart_id
            .checked_mul(size_of::<u64>())
            .and_then(|offset| offset.checked_add(self.offset()))
            .expect("CLINT hart offset overflowed")
    }
}

fn bind(
    registers: DeviceRegisterRange,
    memory: &mut MemoryRegistry,
    hart_id_upper_bound: usize,
) -> Result<Box<dyn TimerDevice>> {
    let mtimecmp_size_bytes = hart_id_upper_bound
        .checked_mul(size_of::<u64>())
        .ok_or(Error::Overflow)?;
    let mtimecmp_registers = registers.subrange(clint::MTIMECMP_OFFSET, mtimecmp_size_bytes)?;
    if !mtimecmp_registers.has_aligned_bounds(align_of::<u32>()) {
        return Err(Error::InvalidArgs);
    }
    let mtimecmp_mmio = memory.acquire(mtimecmp_registers)?;
    Ok(Box::new(THeadTimer::new(mtimecmp_mmio)))
}

struct THeadTimer {
    mtimecmp: MmioRegion,
}

impl THeadTimer {
    fn new(mtimecmp: MmioRegion) -> Self {
        Self { mtimecmp }
    }

    fn write(&self, reg: TimerRegister, hart_id: usize, value: u32) {
        self.mtimecmp
            .write(reg.offset_for_hart(hart_id), value)
            .expect("BUG: T-Head CLINT timer register escaped its MMIO window")
    }
}

impl TimerDevice for THeadTimer {
    #[inline(always)]
    fn set_compare(&self, hart: HartId, value: u64) {
        let hart_id = hart.as_usize();
        let low = value as u32;
        let high = (value >> u32::BITS) as u32;

        // Prevent an interrupt while replacing the two halves: raise the
        // temporary compare value first, then install the final high and low
        // words in the order used by OpenSBI's 32-bit MTIMER accessor.
        self.write(TimerRegister::MtimecmpLow, hart_id, u32::MAX);
        self.write(TimerRegister::MtimecmpHigh, hart_id, high);
        self.write(TimerRegister::MtimecmpLow, hart_id, low);
    }
}
