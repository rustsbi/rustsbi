//! SiFive test finisher: board exit and reset signaling.
//!
//! # References
//!
//! - Platform source: [QEMU SiFive test-finisher implementation](https://github.com/qemu/qemu/blob/99e54ab5e7a6efc945af6d5661842155d1f3fc7a/hw/misc/sifive_test.c) —
//!   finisher register values and exit-code encoding.

use alloc::boxed::Box;
use core::mem::{align_of, size_of};
use runtime::{
    Error, PlatformView, Result,
    memory::{DeviceRegisterRange, MemoryRegistry, MmioRegion},
};

use crate::devicetree::EnabledNode;
use crate::driver::device::Driver;

use super::{Description, Reset, ResetDevice, ResetError, ResetReason, ResetRequest, ResetType};

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

    fn for_request(req: ResetRequest) -> Option<Self> {
        match (req.reset_type(), req.reset_reason()) {
            (ResetType::Shutdown, ResetReason::NoReason) => Some(Self::new(FinishAction::Pass, 0)),
            (ResetType::Shutdown, ResetReason::SystemFailure) => {
                Some(Self::new(FinishAction::Fail, u16::MAX))
            }
            (
                ResetType::ColdReboot | ResetType::WarmReboot,
                ResetReason::NoReason | ResetReason::SystemFailure,
            ) => Some(Self::new(FinishAction::Reset, 0)),
            _ => None,
        }
    }
}

/// SiFive test device used by QEMU to exit or reset.
struct Test {
    registers: MmioRegion,
}

impl Test {
    fn bind(registers: DeviceRegisterRange, memory: &mut MemoryRegistry) -> Result<Self> {
        let registers = registers.subrange(0, SPAN)?;
        if !registers.start().is_aligned_to(align_of::<u32>()) {
            return Err(Error::InvalidArgs);
        }
        Ok(Self {
            registers: memory.acquire(registers)?,
        })
    }

    /// Writes the finish value and parks the hart until the board powers off.
    fn finish(&mut self, command: FinishCommand) -> ResetError {
        if self
            .registers
            .write(Register::Finish.offset(), command.0)
            .is_err()
        {
            return ResetError::Failed;
        }
        loop {
            riscv::asm::wfi();
        }
    }
}

impl ResetDevice for Test {
    fn priority(&self) -> u32 {
        255
    }

    fn system_reset(&mut self, request: ResetRequest) -> ResetError {
        let Some(command) = FinishCommand::for_request(request) else {
            return ResetError::InvalidRequest;
        };
        self.finish(command)
    }
}

/// Driver for the SiFive test finisher.
pub(in crate::driver) struct TestDriver;

pub(super) static TEST: TestDriver = TestDriver;

impl Driver<Reset> for TestDriver {
    fn compatibles(&self) -> &'static [&'static str] {
        &["sifive,test0"]
    }

    fn parse<'a, 'tree>(
        &self,
        platform: &'a PlatformView<'tree>,
        device: EnabledNode<'a, 'tree>,
        _compatible: &str,
    ) -> Result<Option<Description>> {
        let registers = platform
            .device_register(device.node())?
            .ok_or(Error::InvalidArgs)?;
        Ok(Some(Description::new(
            "sifive_test",
            registers,
            move |memory| Ok(Box::new(Test::bind(registers, memory)?) as Box<dyn ResetDevice>),
        )))
    }
}
