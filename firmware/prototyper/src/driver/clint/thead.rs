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
//! [`THeadTimer::set_mtimecmp`] prevents a transient early timer interrupt.

use alloc::boxed::Box;
use core::mem::{align_of, size_of};

use runtime::memory::{DeviceRegisterRange, MemoryRegistry, MmioRegion};

use crate::cfg::NUM_HART_MAX;
use crate::driver::{InterruptDevices, IpiDevice, TimerDevice};

// The ACLINT legacy mapping places MTIMECMP at offset 0x4000.
const MTIMECMP_OFFSET: usize = 0x4000;
const MSIP_WINDOW_SIZE: usize = NUM_HART_MAX * size_of::<u32>();
const MTIMECMP_WINDOW_SIZE: usize = NUM_HART_MAX * size_of::<u64>();

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
        assert!(
            hart_id < NUM_HART_MAX,
            "BUG: T-Head CLINT timer hart index is out of range"
        );
        hart_id * size_of::<u64>() + self.offset()
    }
}

#[repr(usize)]
#[derive(Clone, Copy)]
enum IpiRegister {
    Msip = 0,
}

impl IpiRegister {
    const fn offset(self) -> usize {
        self as usize
    }

    fn offset_for_hart(self, hart_id: usize) -> usize {
        assert!(
            hart_id < NUM_HART_MAX,
            "BUG: T-Head CLINT IPI hart index is out of range"
        );
        self.offset() + hart_id * size_of::<u32>()
    }
}

pub(super) fn bind(
    registers: DeviceRegisterRange,
    memory: &mut MemoryRegistry,
) -> runtime::Result<InterruptDevices> {
    let msip_registers = registers.subrange(0, MSIP_WINDOW_SIZE)?;
    let mtimecmp_registers = registers.subrange(MTIMECMP_OFFSET, MTIMECMP_WINDOW_SIZE)?;
    if !msip_registers.has_aligned_bounds(align_of::<u32>())
        || !mtimecmp_registers.has_aligned_bounds(align_of::<u32>())
    {
        return Err(runtime::Error::InvalidArgs);
    }

    let msip_mmio = memory.acquire_mmio(msip_registers)?;
    let mtimecmp_mmio = memory.acquire_mmio(mtimecmp_registers)?;
    Ok(InterruptDevices {
        timer: Box::new(THeadTimer::new(mtimecmp_mmio)),
        ipi: Box::new(THeadIpi::new(msip_mmio)),
    })
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

    fn set_mtimecmp(&self, hart_id: usize, value: u64) {
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

impl TimerDevice for THeadTimer {
    #[inline(always)]
    fn read_time(&self) -> u64 {
        // T-Head CLINTs have no memory-mapped `mtime`; read the `time` CSR.
        riscv::register::time::read64()
    }

    #[inline(always)]
    fn set_timer(&self, hart_id: usize, value: u64) {
        self.set_mtimecmp(hart_id, value);
    }
}

#[repr(u32)]
enum IpiState {
    Clear = 0,
    Pending = 1,
}

struct THeadIpi {
    msip: MmioRegion,
}

impl THeadIpi {
    fn new(msip: MmioRegion) -> Self {
        Self { msip }
    }

    fn write(&self, reg: IpiRegister, hart_id: usize, value: IpiState) {
        self.msip
            .write(reg.offset_for_hart(hart_id), value as u32)
            .expect("BUG: T-Head CLINT IPI register escaped its MMIO window")
    }
}

impl IpiDevice for THeadIpi {
    #[inline(always)]
    fn send_ipi(&self, hart_id: usize) {
        self.write(IpiRegister::Msip, hart_id, IpiState::Pending)
    }

    #[inline(always)]
    fn clear_ipi(&self) {
        self.write(
            IpiRegister::Msip,
            crate::riscv::current_hartid(),
            IpiState::Clear,
        )
    }
}
