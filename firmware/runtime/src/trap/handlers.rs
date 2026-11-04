//! SBI and machine interrupt handlers selected by trap dispatch.
use super::frame::TrapFrame;
use crate::rustsbi::SbiRet;
use crate::{
    boot::{self, NextStage},
    hart::{self, HartEvent},
    instruction,
    ipi::{self, Ipi},
    irq::LocalIrqDisabled,
};

// The firmware supplies this symbol through `#[runtime::sbi_policy]`.
// Both crates use the same Rust ABI and SBI types in the final firmware.
unsafe extern "Rust" {
    fn __rustsbi_runtime_handle_ecall(
        extension: usize,
        function: usize,
        args: [usize; 6],
    ) -> SbiRet;
}

pub(super) fn sbi_ecall(frame: &mut TrapFrame) {
    let regs = &mut frame.general;
    let hart = hart::current_hart();
    // SAFETY: The firmware's `sbi_policy` symbol has the declared ABI and
    // handles ecalls only after its policy state has been initialized.
    let ret = unsafe {
        __rustsbi_runtime_handle_ecall(
            regs.a7,
            regs.a6,
            [regs.a0, regs.a1, regs.a2, regs.a3, regs.a4, regs.a5],
        )
    };
    regs.a0 = ret.error;
    regs.a1 = ret.value;
    frame.epc = frame.epc.wrapping_add(4);

    if let Some(hart::ControlTransfer::NonRetentiveResume(next)) = hart::take_control_transfer(hart)
    {
        enter_next_stage(frame, next);
    }
}

fn deliver_event(frame: &mut TrapFrame, ipi: Option<&Ipi>, irq: &LocalIrqDisabled) {
    let event = hart::take_local_event();
    if let Some(ipi) = ipi {
        ipi.process_pending(irq);
    }
    match event {
        HartEvent::Start(next) => enter_next_stage(frame, next),
        HartEvent::Park => {
            if let Some(ipi) = ipi {
                ipi.enable_interrupt(irq);
            }
            instruction::wait_for_interrupt();
        }
        HartEvent::None => {}
    }
}

pub(super) fn software_interrupt(frame: &mut TrapFrame, irq: &LocalIrqDisabled) {
    let ipi = ipi::get()
        .filter(|ipi| !ipi.is_external())
        .and_then(|device| {
            let acknowledged = device
                .acknowledge(irq)
                .expect("IPI device could not acknowledge the current hart");
            acknowledged.then_some(device)
        });
    deliver_event(frame, ipi, irq);
}

pub(super) fn external_interrupt(frame: &mut TrapFrame, irq: &LocalIrqDisabled) {
    if let Some(device) = ipi::get().filter(|ipi| ipi.is_external())
        && device
            .acknowledge(irq)
            .expect("IPI device could not acknowledge the current hart")
    {
        deliver_event(frame, Some(device), irq);
    }
}

fn enter_next_stage(frame: &mut TrapFrame, next: NextStage) {
    // SAFETY: Dispatch owns this hart's trap state, with MIE masked.
    unsafe { boot::prepare_supervisor() };
    boot::set_return_mode(&mut frame.status, next.next_mode);
    frame.leave_virtualization();
    frame.epc = next.start_addr;
    frame.general = Default::default();
    frame.general.a0 = hart::current_hart().as_usize();
    frame.general.a1 = next.opaque;
    instruction::fence_i();
}
