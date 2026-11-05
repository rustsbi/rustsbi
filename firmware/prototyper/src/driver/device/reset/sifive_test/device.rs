//! SiFive test finisher MMIO device.

use core::{
    convert::Infallible,
    mem::{align_of, size_of},
};
use runtime::{
    Error, Result,
    memory::{MemoryRegistry, MmioRegion},
};

use super::super::{ResetDevice, ResetReason, ResetRequest, ResetType};
use super::description::Desc;

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

/// A finisher action written in the low half-word.
#[repr(u16)]
enum FinishAction {
    Fail = 0x3333,
    Pass = 0x5555,
    Reset = 0x7777,
}

/// A finisher command carrying an action and an exit code.
struct FinishCommand(u32);

impl FinishCommand {
    fn new(action: FinishAction, code: u16) -> Self {
        // The finisher reads the action from the low half-word and the exit
        // code from the high half-word.
        Self(u32::from(action as u16) | (u32::from(code) << u16::BITS))
    }

    fn for_request(req: ResetRequest) -> Option<Self> {
        // Report a system-failure shutdown through the fail finisher;
        // a normal shutdown uses pass.
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
pub(super) struct SifiveTestDevice {
    registers: MmioRegion,
}

impl SifiveTestDevice {
    pub(super) fn init(description: Desc, memory: &mut MemoryRegistry) -> Result<Self> {
        let Desc { registers } = description;
        let registers = registers.subrange(0, SPAN)?;
        if !registers.start().is_aligned_to(align_of::<u32>()) {
            return Err(Error::InvalidArgs);
        }
        Ok(Self {
            registers: memory.acquire(registers)?,
        })
    }

    /// Issues a finisher command and waits for reset or poweroff.
    fn finish(&mut self, command: FinishCommand) -> Result<Infallible> {
        if self
            .registers
            .write(Register::Finish.offset(), command.0)
            .is_err()
        {
            return Err(Error::IoError);
        }
        loop {
            runtime::instruction::wait_for_interrupt();
        }
    }
}

impl ResetDevice for SifiveTestDevice {
    fn system_reset(&mut self, request: ResetRequest) -> Result<Infallible> {
        let Some(command) = FinishCommand::for_request(request) else {
            return Err(Error::InvalidArgs);
        };
        self.finish(command)
    }
}
