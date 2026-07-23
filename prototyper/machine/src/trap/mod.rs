//! Closed machine-trap routing and the narrow SBI call boundary.
//!
//! Trap frames, restoration, redirection and illegal-instruction emulation
//! remain private. Upper firmware receives copied SBI arguments and returns
//! only the two architectural result registers.

mod arch;
mod delegation;
mod dispatch;
pub(crate) mod expected;
mod features;
mod frame;
mod illegal;
mod redirect;
mod stack;

pub(crate) use arch::{
    activate, current_index, enter_resumed_stage, hypervisor_available, install, park_current_hart,
    prepare_counters, prepare_hypervisor_metadata, prepare_timer,
};
pub(crate) use delegation::prepare as prepare_delegation;

pub(crate) fn abort() -> ! {
    crate::power::abort(|| {})
}

use frame::Frame;
#[cfg(test)]
use frame::HypervisorTrap;
use illegal::decode_time_read;
use illegal::{DecodedTimeRead, TimeCsr};
#[cfg(test)]
use redirect::hypervisor_status;
use redirect::{read_supervisor_vector, write_supervisor_trap};

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

/// The two registers returned by an SBI environment call.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SbiResponse {
    /// SBI error code returned in `a0`.
    pub error: usize,
    /// SBI result value returned in `a1`.
    pub value: usize,
}

impl SbiResponse {
    /// Creates one copied SBI response.
    pub const fn new(error: usize, value: usize) -> Self {
        Self { error, value }
    }
}

/// A lower-privilege trap observed before machine-owned routing completes.
///
/// This value carries no frame, register, CSR or continuation authority.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TrapEvent {
    /// An illegal instruction was trapped for compatibility emulation.
    IllegalInstruction,
    /// A misaligned load is being redirected to the supervisor.
    MisalignedLoad,
    /// A misaligned store or atomic operation is being redirected.
    MisalignedStore,
}

/// Safe upper policy for copied SBI calls and optional trap accounting.
pub trait SbiHandler: Send + Sync + 'static {
    /// Handles one copied SBI call and returns the two SBI result registers.
    fn handle_ecall(&self, call: SbiCall) -> SbiResponse;

    /// Observes a trap event without receiving its frame or continuation.
    fn observe_trap(&self, _event: TrapEvent) {}
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
        arch::restore(self)
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
        let hypervisor = if features::hypervisor_metadata_available(
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
        arch::restore(self)
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
        arch::restore(self)
    }
}

#[cfg(test)]
mod tests;
