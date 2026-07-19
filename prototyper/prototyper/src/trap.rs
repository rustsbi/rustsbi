//! Upper machine-trap policy.

use machine::{Cause, Trap};
use rustsbi::RustSBI;
use sbi_spec::pmu::firmware_event;

use crate::sbi;

/// The sole upper trap handler installed for every admitted hart.
pub struct Handler {
    sbi: sbi::Handler,
}

impl Handler {
    /// Creates the trap policy around the complete SBI protocol handler.
    pub fn new(sbi: sbi::Handler) -> Self {
        Self { sbi }
    }
}

impl machine::TrapHandler for Handler {
    fn handle(&self, trap: Trap<'_>) -> ! {
        match trap.cause() {
            Cause::SbiCall {
                extension_id,
                function_id,
                arguments,
            } => {
                let result = self.sbi.handle_ecall(extension_id, function_id, arguments);
                self.sbi.record_sbi_call(extension_id, function_id);
                trap.resume_from_ecall(result.error, result.value)
            }
            Cause::IllegalInstruction => {
                self.sbi.record_firmware_event(firmware_event::ILLEGAL_INSN);
                trap.emulate_illegal()
            }
            Cause::LoadMisaligned => {
                self.sbi
                    .record_firmware_event(firmware_event::MISALIGNED_LOAD);
                trap.redirect()
            }
            Cause::StoreMisaligned => {
                self.sbi
                    .record_firmware_event(firmware_event::MISALIGNED_STORE);
                trap.redirect()
            }
            Cause::Other => trap.redirect(),
            Cause::MachineSoftwareInterrupt
            | Cause::MachineTimerInterrupt
            | Cause::MachineExternalInterrupt => machine::abort(|| {}),
        }
    }
}
