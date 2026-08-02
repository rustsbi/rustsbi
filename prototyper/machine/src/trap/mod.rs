//! Closed machine-trap routing and the narrow SBI call boundary.
//!
//! Trap frames, restoration, redirection and illegal-instruction emulation
//! remain private. Upper firmware receives copied SBI arguments and returns
//! only the two architectural result registers.

mod delegation;
mod dispatch;
mod entry;
mod frame;
pub(crate) mod probe;
mod redirect;
mod stack;

pub(crate) use delegation::prepare as prepare_delegation;
pub(crate) use entry::{
    activate, current_index, enter_resumed_stage, hypervisor_available, install, park_current_hart,
    prepare_counters, prepare_hypervisor_metadata, prepare_timer,
};

pub(crate) fn abort() -> ! {
    crate::power::abort(|| {})
}

use frame::Frame;
#[cfg(test)]
use frame::HypervisorTrap;
#[cfg(test)]
use redirect::hypervisor_status;
use redirect::{read_supervisor_vector, write_supervisor_trap};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TimeCsr {
    Time,
    TimeHigh,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct DecodedTimeRead {
    destination_register: usize,
    csr: TimeCsr,
}

fn decode_time_read(instruction: usize) -> Option<DecodedTimeRead> {
    const OPCODE_SYSTEM: usize = 0x73;
    const FUNCT3_CSRRS: usize = 0b010;
    const CSR_TIME: usize = 0xc01;
    const CSR_TIMEH: usize = 0xc81;

    let instruction = u32::try_from(instruction).ok()? as usize;
    if instruction & 0x7f != OPCODE_SYSTEM
        || (instruction >> 12) & 0b111 != FUNCT3_CSRRS
        || (instruction >> 15) & 0b1_1111 != 0
    {
        return None;
    }
    let destination_register = (instruction >> 7) & 0b1_1111;
    let csr = match (instruction >> 20) & 0xfff {
        CSR_TIME => TimeCsr::Time,
        CSR_TIMEH => TimeCsr::TimeHigh,
        _ => return None,
    };
    Some(DecodedTimeRead {
        destination_register,
        csr,
    })
}

/// One SBI environment call copied out of the private machine trap frame.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SbiCall {
    /// SBI extension identifier copied from `a7`.
    pub extension_id: usize,
    /// SBI function identifier copied from `a6`.
    pub function_id: usize,
    /// SBI arguments copied from `a0..a5`.
    pub arguments: [usize; 6],
}

/// Safe upper policy for copied SBI environment calls.
pub trait SbiHandler: Send + Sync + 'static {
    /// Handles one copied SBI call and returns the standard SBI result pair.
    fn handle(&self, call: SbiCall) -> sbi_spec::binary::SbiRet;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Cause {
    SbiCall(SbiCall),
    /// A machine software interrupt.
    MachineSoftwareInterrupt,
    /// A machine timer interrupt.
    MachineTimerInterrupt,
    /// A machine external interrupt.
    MachineExternalInterrupt,
    /// An illegal instruction exception.
    IllegalInstruction,
    /// A misaligned load exception.
    LoadMisaligned,
    /// A misaligned store or atomic-memory-operation exception.
    StoreMisaligned,
    /// Another lower-mode trap that may be redirected by machine policy.
    Other,
}

/// Exclusive authority over one fully initialized machine trap frame.
///
/// The value cannot outlive or alias its private frame. It deliberately has no
/// general register or CSR mutation API; terminal operations expose only the
/// architectural commits required by upper trap policy.
struct Trap<'frame> {
    frame: &'frame mut Frame,
    stack_top: usize,
}

impl Trap<'_> {
    #[cfg(test)]
    fn cause(&self) -> Cause {
        self.frame.cause()
    }

    /// Commits the two SBI result registers, advances past the active
    /// supervisor ecall, and returns with `mret`.
    fn resume_from_ecall(self, a0: usize, a1: usize) -> ! {
        if !matches!(self.frame.cause(), Cause::SbiCall(_))
            || !self.frame.set_register(10, a0)
            || !self.frame.set_register(11, a1)
            || !self.frame.advance_pc(4)
        {
            abort();
        }
        entry::restore(self)
    }

    /// Redirects the unchanged active lower-mode trap to the supervisor vector.
    fn redirect(self) -> ! {
        if matches!(
            self.frame.cause(),
            Cause::SbiCall(_)
                | Cause::MachineSoftwareInterrupt
                | Cause::MachineTimerInterrupt
                | Cause::MachineExternalInterrupt
        ) || self.frame.previous_mode() > 1
        {
            abort();
        }

        let supervisor_pc = self.frame.pc();
        let supervisor_cause = self.frame.encoded_cause();
        let supervisor_value = self.frame.trap_value();
        let hypervisor = if probe::hypervisor_metadata_available(
            stack::index_for_top(self.stack_top).unwrap_or_else(|| abort()),
        ) {
            match self.frame.hypervisor_trap() {
                Some(metadata) => Some(metadata),
                None => abort(),
            }
        } else {
            None
        };
        let Some(supervisor_vector) = read_supervisor_vector() else {
            abort();
        };
        let mode = supervisor_vector & 0b11;
        let entry = supervisor_vector & !0b11;
        if mode > 1 || !self.frame.redirect_to_supervisor(entry) {
            abort();
        }
        if !write_supervisor_trap(
            supervisor_pc,
            supervisor_cause,
            supervisor_value,
            hypervisor,
        ) {
            abort();
        }
        entry::restore(self)
    }

    /// Emulates the retained read-only time CSR compatibility case.
    ///
    /// Every other illegal instruction is redirected unchanged.
    fn emulate_illegal(self) -> ! {
        let instruction = self.frame.trap_value();
        let Some(DecodedTimeRead {
            destination_register,
            csr,
        }) = decode_time_read(instruction)
        else {
            // TODO: Broader illegal-instruction compatibility requires a
            // separate review of instruction fetch, CSR permissions,
            // secondary faults, and architectural commit.
            self.redirect();
        };
        if !matches!(self.frame.cause(), Cause::IllegalInstruction) {
            abort();
        }
        let Some(time) = crate::timer::read_time() else {
            self.redirect();
        };
        let value = match csr {
            TimeCsr::Time => time as usize,
            TimeCsr::TimeHigh => {
                #[cfg(target_pointer_width = "32")]
                {
                    (time >> 32) as usize
                }
                #[cfg(target_pointer_width = "64")]
                {
                    self.redirect();
                }
            }
        };
        if !self.frame.commit_instruction(destination_register, value) {
            abort();
        }
        entry::restore(self)
    }
}

#[cfg(test)]
mod tests;
