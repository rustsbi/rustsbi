//! SiFive test finisher: board exit and reset signaling.
//!
//! # References
//!
//! - Platform source: [QEMU SiFive test-finisher implementation](https://github.com/qemu/qemu/blob/99e54ab5e7a6efc945af6d5661842155d1f3fc7a/hw/misc/sifive_test.c) —
//!   finisher register values and exit-code encoding.

use alloc::boxed::Box;
use core::mem::{align_of, size_of};
use runtime::memory::{DeviceRegisterRange, MemoryRegistry, MmioRegion};

use crate::driver::reset::ResetDevice;

#[repr(usize)]
#[derive(Clone, Copy)]
enum Register {
    Finish = 0x0000,
}

impl Register {
    const fn offset(self) -> usize {
        self as usize
    }
}

const SPAN: usize = Register::Finish.offset() + size_of::<u32>();

#[repr(u16)]
enum FinishAction {
    Fail = 0x3333,
    Pass = 0x5555,
    Reset = 0x7777,
}

struct FinishCommand(u32);

impl FinishCommand {
    fn new(action: FinishAction, code: u16) -> Self {
        Self(u32::from(action as u16) | (u32::from(code) << u16::BITS))
    }
}

/// SiFive test device used by QEMU to exit or reset.
struct SifiveTestDevice {
    registers: MmioRegion,
}

pub(super) fn bind(
    registers: DeviceRegisterRange,
    memory: &mut MemoryRegistry,
) -> runtime::Result<Box<dyn ResetDevice>> {
    let registers = registers.subrange(0, SPAN)?;
    if !registers.start().is_aligned_to(align_of::<u32>()) {
        return Err(runtime::Error::InvalidArgs);
    }
    Ok(Box::new(SifiveTestDevice::new(
        memory.acquire_mmio(registers)?,
    )))
}

impl SifiveTestDevice {
    /// Creates a reset device from its acquired register window.
    fn new(registers: MmioRegion) -> Self {
        Self { registers }
    }

    /// Writes the finish value and parks the hart until the board powers off.
    fn finish(&self, command: FinishCommand) -> ! {
        self.registers
            .write(Register::Finish.offset(), command.0)
            .expect("BUG: SiFive test register escaped its MMIO window");
        loop {
            riscv::asm::wfi();
        }
    }
}

impl ResetDevice for SifiveTestDevice {
    #[inline]
    fn fail(&self, code: u16) -> ! {
        self.finish(FinishCommand::new(FinishAction::Fail, code))
    }

    #[inline]
    fn pass(&self) -> ! {
        self.finish(FinishCommand::new(FinishAction::Pass, 0))
    }

    #[inline]
    fn reset(&self) -> ! {
        self.finish(FinishCommand::new(FinishAction::Reset, 0))
    }
}
