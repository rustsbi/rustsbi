//! SBI and machine interrupt handlers selected by trap dispatch.
use super::{frame::TrapFrame, init};
use crate::{
    boot::NextStage,
    hart::{self, HartEvent},
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

    if let Some(hart::ControlTransfer::NonRetentiveResume(next)) = hart::take_control_transfer(hart)
    {
        enter_next_stage(frame, next);
    }
}

fn deliver_event(frame: &mut TrapFrame) {
    let event = hart::take_local_event();
    crate::ipi::handler()
        .expect("IPI handler not published")
        .deliver_current();
    match event {
        HartEvent::Start(next) => enter_next_stage(frame, next),
        HartEvent::Park => {
            crate::csr::mie::set_machine_software();
            if crate::irq::get().is_some() {
                crate::csr::mie::set_machine_external();
            }
            riscv::asm::wfi();
        }
        HartEvent::None => {}
    }
}

pub(super) fn software_interrupt(frame: &mut TrapFrame) {
    crate::ipi::get()
        .expect("software interrupt without an IPI device")
        .clear_current()
        .expect("IPI backend could not clear the current hart");
    deliver_event(frame);
}

pub(super) fn external_interrupt(frame: &mut TrapFrame) {
    if crate::irq::get().is_some_and(|controller| controller.claim_ipi()) {
        deliver_event(frame);
    }
}

pub(super) fn timer_interrupt() {
    crate::csr::mie::clear_machine_timer();
    if !crate::timer::has_sstc() {
        if let Some(timer) = crate::timer::get() {
            timer.acknowledge_current();
        }
        crate::csr::mip::set_supervisor_timer();
    }
}

fn enter_next_stage(frame: &mut TrapFrame, next: NextStage) {
    // SAFETY: Dispatch owns this hart's trap state, with MIE masked.
    unsafe { crate::boot::prepare_supervisor() };
    crate::boot::set_return_mode(&mut frame.status, next.next_mode);
    frame.leave_virtualization();
    frame.epc = next.start_addr;
    frame.general = Default::default();
    frame.general.a0 = hart::current_hart().as_usize();
    frame.general.a1 = next.opaque;
    riscv::asm::fence_i();
}
