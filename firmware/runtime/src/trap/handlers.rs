//! SBI and machine interrupt handlers selected by trap dispatch.
use super::{frame::TrapFrame, init};
use crate::{
    boot::NextStage,
    hart::{self, Hart, HartEvent},
    irq::LocalIrqDisabled,
};

pub(super) fn sbi_ecall(frame: &mut TrapFrame) {
    let regs = &mut frame.general;
    let hart = hart::current_hart();
    let ret = init::policy(hart).handle_ecall(
        regs.a7,
        regs.a6,
        [regs.a0, regs.a1, regs.a2, regs.a3, regs.a4, regs.a5],
    );
    regs.a0 = ret.error;
    regs.a1 = ret.value;
    frame.epc = frame.epc.wrapping_add(4);

    let current = Hart::get(hart).expect("dispatch hart");
    match current.take_control_transfer() {
        Some(hart::ControlTransfer::Stop) => {
            // SAFETY: Machine trap dispatch keeps MIE clear through this wait.
            let irq = unsafe { LocalIrqDisabled::assume_disabled() };
            enter_next_stage(frame, current.wait_for_start(&irq));
        }
        Some(hart::ControlTransfer::NonRetentiveResume(next)) => {
            crate::pmu::Pmu::reset_current().expect("committed PMU resume reset failed");
            init::policy(hart).on_nonretentive_resume();
            enter_next_stage(frame, next);
        }
        None => {}
    }
}

fn deliver_event(frame: &mut TrapFrame, ipi: &crate::ipi::Ipi, irq: &LocalIrqDisabled) {
    let event = Hart::current().expect("dispatch hart").take_local_event();
    ipi.deliver_current(irq);
    match event {
        HartEvent::Start(next) => enter_next_stage(frame, next),
        HartEvent::Park => {
            ipi.prepare_wakeup(irq);
            riscv::asm::wfi();
        }
        HartEvent::None => {}
    }
}

pub(super) fn software_interrupt(frame: &mut TrapFrame, irq: &LocalIrqDisabled) {
    let ipi = crate::boot::ipi().expect("IPI service not published");
    ipi.clear_current(irq)
        .expect("IPI device could not clear the current hart");
    deliver_event(frame, ipi, irq);
}

pub(super) fn external_interrupt(frame: &mut TrapFrame, irq: &LocalIrqDisabled) {
    if let Some(ipi) = crate::boot::ipi()
        && ipi.claim_external(irq)
    {
        deliver_event(frame, ipi, irq);
    }
}

fn enter_next_stage(frame: &mut TrapFrame, next: NextStage) {
    // SAFETY: Dispatch owns this hart's trap state, with MIE masked.
    unsafe { crate::boot::prepare_supervisor() };
    crate::boot::set_return_mode(&mut frame.status, next.next_mode());
    frame.leave_virtualization();
    frame.epc = next.start_addr();
    frame.general = Default::default();
    frame.general.a0 = hart::current_hart().as_usize();
    frame.general.a1 = next.opaque();
    riscv::asm::fence_i();
}
