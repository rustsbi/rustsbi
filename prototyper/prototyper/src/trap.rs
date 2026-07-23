//! Upper SBI call handling and firmware-event accounting.

use machine::{SbiCall, SbiResponse, TrapEvent};
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

impl machine::SbiHandler for Handler {
    fn handle_ecall(&self, call: SbiCall) -> SbiResponse {
        let result = self
            .sbi
            .handle_ecall(call.extension_id, call.function_id, call.arguments);
        self.sbi
            .record_sbi_call(call.extension_id, call.function_id);
        SbiResponse::new(result.error, result.value)
    }

    fn observe_trap(&self, event: TrapEvent) {
        match event {
            TrapEvent::IllegalInstruction => {
                self.sbi.record_firmware_event(firmware_event::ILLEGAL_INSN);
            }
            TrapEvent::MisalignedLoad => {
                self.sbi
                    .record_firmware_event(firmware_event::MISALIGNED_LOAD);
            }
            TrapEvent::MisalignedStore => {
                self.sbi
                    .record_firmware_event(firmware_event::MISALIGNED_STORE);
            }
        }
    }
}
