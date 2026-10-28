//! SBI and machine interrupt handlers selected by trap dispatch.
use super::{frame::TrapFrame, init};
use crate::{
    boot::NextStage,
    context::{self, ExecutionContext},
    csr::csrr,
    hart::{self, HartEvent},
    irq::LocalIrqDisabled,
};
use csr::{
    machine::{MPP, TargetPrivilege},
    supervisor,
    values::trap::InterruptSet,
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

    match hart::take_control_transfer(hart) {
        Some(hart::ControlTransfer::NonRetentiveResume(next)) => enter_next_stage(frame, next),
        Some(hart::ControlTransfer::Retentive {
            suspend_into,
            resume_from,
        }) => enter_context(frame, suspend_into, resume_from),
        None => {}
    }
}

fn deliver_event(frame: &mut TrapFrame, ipi: &crate::ipi::Ipi, irq: &LocalIrqDisabled) {
    let event = hart::take_local_event();
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
    // The fresh stage is not a tracked execution context.
    hart::set_active_context(None);
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

fn enter_context(
    frame: &mut TrapFrame,
    suspend_into: &'static ExecutionContext,
    resume_from: &'static ExecutionContext,
) {
    // Save the outgoing execution before any live state is replaced. The
    // ecall advance has already committed the outgoing resume PC, and the
    // frame's a0/a1 carry the ecall return values.
    let mut x1_x31 = [0usize; 31];
    for (slot, id) in x1_x31.iter_mut().zip(1u8..=31) {
        *slot = frame.general.read(id);
    }
    let outgoing = riscv::register::satp::read();
    // A supervisor context always has a `sie`; a failed read resumes the
    // incoming context with no interrupt sources rather than failing the
    // infallible ceremony.
    let outgoing_sie = csrr!(supervisor::InterruptEnable)
        .map(InterruptSet::from)
        .unwrap_or(InterruptSet::empty());
    let outgoing_status = frame.status;
    suspend_into.save_outgoing(&x1_x31, frame.epc, outgoing, outgoing_sie, outgoing_status);
    // SAFETY: M-mode rewrites this hart's supervisor environment for the
    // staged transfer.
    unsafe { crate::boot::prepare_supervisor() };
    let incoming = resume_from.satp();
    // SAFETY: M-mode installs the staged transfer's declared satp.
    unsafe { riscv::register::satp::write(incoming) };
    // Fence only on an actual address-space change, a reused ASID included.
    if incoming != outgoing {
        riscv::asm::sfence_vma_all();
    }
    // SAFETY: M-mode installs the staged transfer's declared interrupt
    // enables, replacing prepare_supervisor's clearing.
    unsafe { supervisor::InterruptEnable::write(resume_from.sie()) };
    frame.status = resume_from.status();
    // The staged return state is the context's own, but a transfer always
    // re-enters supervisor mode.
    frame.status.write::<MPP>(TargetPrivilege::Supervisor);
    frame.leave_virtualization();
    let entry_pc = resume_from.resume_into(&mut x1_x31);
    for (id, value) in x1_x31.iter().enumerate() {
        frame.general.write(id as u8 + 1, *value);
    }
    frame.epc = entry_pc;
    riscv::asm::fence_i();
    context::commit_pair(suspend_into, resume_from);
    hart::set_active_context(Some(resume_from));
}
