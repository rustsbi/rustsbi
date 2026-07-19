//! Typed ownership of one complete machine trap.
//!
//! The private frame is fully initialized by target entry assembly before a
//! `Trap` is formed. Upper policy receives only this consuming, lifetime-bound
//! view and cannot retain the frame or mutate arbitrary registers.

#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
mod dispatch;
pub(crate) mod entry;
pub(crate) mod expected;
mod frame;
mod illegal;
mod redirect;

pub(crate) use entry::current_index;

pub(crate) fn abort() -> ! {
    entry::abort()
}

use entry::HartTrapState;
use frame::Frame;
#[cfg(test)]
use frame::HypervisorTrap;
use illegal::{DecodedTimeRead, TimeCsr, decode_time_read};
#[cfg(test)]
use redirect::hypervisor_status;
use redirect::{read_supervisor_vector, write_supervisor_trap};

/// The architectural reason for the current machine trap.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Cause {
    /// A supervisor binary-interface call with copied ABI register values.
    SbiCall {
        /// SBI extension identifier from `a7`.
        extension_id: usize,
        /// SBI function identifier from `a6`.
        function_id: usize,
        /// SBI arguments copied from `a0..a5`.
        arguments: [usize; 6],
    },
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
pub struct Trap<'frame> {
    frame: &'frame mut Frame,
    state: &'frame HartTrapState,
}

impl Trap<'_> {
    /// Returns the decoded interrupt or exception cause.
    pub fn cause(&self) -> Cause {
        self.frame.cause()
    }

    /// Restores the unchanged interrupted context and returns with `mret`.
    pub fn resume(self) -> ! {
        entry::restore(self)
    }

    /// Commits the two SBI result registers, advances past the active
    /// supervisor ecall, and returns with `mret`.
    pub fn resume_from_ecall(self, a0: usize, a1: usize) -> ! {
        if !matches!(self.frame.cause(), Cause::SbiCall { .. })
            || !self.frame.set_register(10, a0)
            || !self.frame.set_register(11, a1)
            || !self.frame.advance_pc(4)
        {
            entry::abort();
        }
        entry::restore(self)
    }

    /// Redirects the unchanged active lower-mode trap to the supervisor vector.
    pub fn redirect(self) -> ! {
        if matches!(
            self.frame.cause(),
            Cause::SbiCall { .. }
                | Cause::MachineSoftwareInterrupt
                | Cause::MachineTimerInterrupt
                | Cause::MachineExternalInterrupt
        ) || self.frame.previous_mode() > 1
        {
            entry::abort();
        }

        let supervisor_pc = self.frame.pc();
        let supervisor_cause = self.frame.encoded_cause();
        let supervisor_value = self.frame.trap_value();
        let hypervisor = if self.state.has_hypervisor_metadata() {
            match self.frame.hypervisor_trap() {
                Some(metadata) => Some(metadata),
                None => entry::abort(),
            }
        } else {
            None
        };
        let Some(supervisor_vector) = read_supervisor_vector() else {
            entry::abort();
        };
        let mode = supervisor_vector & 0b11;
        let entry = supervisor_vector & !0b11;
        if mode > 1 || !self.frame.redirect_to_supervisor(entry) {
            entry::abort();
        }
        if !write_supervisor_trap(
            supervisor_pc,
            supervisor_cause,
            supervisor_value,
            hypervisor,
        ) {
            entry::abort();
        }
        entry::restore(self)
    }

    /// Emulates the retained read-only time CSR compatibility case.
    ///
    /// Every other illegal instruction is redirected unchanged.
    pub fn emulate_illegal(self) -> ! {
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
            entry::abort();
        }
        let Some(time) = self.state.read_time() else {
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
            entry::abort();
        }
        entry::restore(self)
    }
}

/// Safe upper policy invoked for an ordinary machine trap.
///
/// Implementations must consume the supplied trap. The machine layer retains
/// frame allocation, nested-entry policy, restoration, and `mret` authority.
pub trait TrapHandler: Send + Sync + 'static {
    /// Handles one complete trap and does not return to the dispatcher.
    fn handle(&self, trap: Trap<'_>) -> !;
}

#[cfg(test)]
mod tests;
