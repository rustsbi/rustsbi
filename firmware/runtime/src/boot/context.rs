//! Trap installation and next-stage architectural context.
use crate::trap::entry;
use csr::{
    machine, supervisor,
    values::{
        counter::CounterSet,
        trap::{Exception, ExceptionSet, InterruptSet},
        vector::{TargetVectorMode, VectorBase},
    },
};

/// Installs machine exception handling before platform feature probes.
///
/// # Safety
///
/// Run in M-mode with a valid Runtime stack, local machine interrupts
/// disabled, and `mscratch` set to zero.
#[doc(hidden)]
pub unsafe extern "C" fn prepare_traps() {
    // SAFETY: Startup owns this hart's trap state before entering policy.
    unsafe {
        install(entry::trap_entry);
        machine::Status::modify(|_, status| {
            status.write::<machine::MPRV>(machine::AccessPrivilege::Current);
            #[cfg(target_pointer_width = "64")]
            status.write::<machine::MDT>(machine::DoubleTrapState::Clear);
        });
        // Install the basic vector before probing so an absent mstatush can recover.
        #[cfg(target_pointer_width = "32")]
        let high = crate::csr::read::<machine::StatusHigh>().ok();
        #[cfg(target_pointer_width = "32")]
        if high.is_some() {
            machine::StatusHigh::modify(|_, status| {
                status.write::<machine::MDT>(machine::DoubleTrapState::Clear);
            });
            install(entry::trap_entry_high);
        }
        if machine::Isa::read().has_hypervisor() {
            install(entry::trap_entry_hypervisor);
        }
    }
}

/// Installs a machine trap vector for the current hart.
///
/// # Safety
///
/// Run in M-mode with machine interrupts disabled.
/// `entry` must implement Runtime's stack and register-preservation protocol.
/// Every optional CSR used by that entry must exist on this hart.
unsafe fn install(entry: unsafe extern "C" fn()) {
    let base = VectorBase::new((entry as *const () as usize).into()).expect("aligned trap vector");
    // SAFETY: The caller supplies a Runtime trap entry.
    unsafe {
        machine::TrapVector::write(machine::TrapVectorWriteValue::new(
            base,
            TargetVectorMode::Direct,
        ))
    }
}

/// Configures delegation and counter access before publishing the SBI policy.
///
/// # Safety
///
/// Run on the current hart in M-mode with machine interrupts disabled,
/// Runtime trap entry installed, and no active lower-mode execution.
pub unsafe fn configure_next_stage() {
    machine::InterruptDelegation::write(usize::MAX.into());
    machine::ExceptionDelegation::write(ExceptionSet::except(&[
        Exception::SupervisorEcall,
        Exception::LoadMisaligned,
        Exception::StoreMisaligned,
        Exception::IllegalInstruction,
        Exception::LoadAccess,
        Exception::StoreAccess,
    ]));
    machine::CounterEnable::write(CounterSet::all());
    supervisor::CounterEnable::write(CounterSet::all());
}

pub(crate) fn set_return_mode(
    status: &mut machine::StatusWriteValue,
    mode: riscv::register::mstatus::MPP,
) {
    status.write::<machine::MPP>(privilege(mode));
    status.write::<machine::MPIE>(machine::InterruptState::Enabled);
    status.write::<machine::SIE>(machine::InterruptState::Disabled);
    #[cfg(target_pointer_width = "64")]
    status.write::<machine::MPV>(machine::Virtualization::Off);
}

fn privilege(mode: riscv::register::mstatus::MPP) -> machine::TargetPrivilege {
    match mode {
        riscv::register::mstatus::MPP::User => machine::TargetPrivilege::User,
        riscv::register::mstatus::MPP::Supervisor => machine::TargetPrivilege::Supervisor,
        riscv::register::mstatus::MPP::Machine => machine::TargetPrivilege::Machine,
    }
}

/// Prepares translation and interrupt state for a new supervisor stage.
///
/// # Safety
///
/// Run in M-mode with interrupts disabled, `MPRV` clear, and Runtime trap entry installed.
/// The outgoing supervisor address space must no longer be needed.
pub(crate) unsafe fn prepare_supervisor() {
    // SAFETY: The caller is replacing the outgoing supervisor context;
    // clearing its translation and interrupt enables cannot invalidate M-mode state.
    unsafe {
        supervisor::InterruptEnable::write(InterruptSet::empty());
        supervisor::AddressTranslation::write(supervisor::AddressSpace::bare());
    }
    crate::csr::mie::set_machine_software();
    if crate::irq::get().is_some() {
        crate::csr::mie::set_machine_external();
    }
    if !crate::timer::has_sstc() {
        crate::csr::mie::set_machine_timer();
    }
}

/// Programs the initial boot handoff in the current hart's CSRs.
// Trap handoffs instead update the saved frame restored by entry.
///
/// # Safety
///
/// M-mode on the current hart, interrupts disabled, no live lower context.
pub(super) unsafe fn stage_next_mode(start_addr: usize, next_mode: riscv::register::mstatus::MPP) {
    // SAFETY: The boot caller owns the outgoing trap state.
    unsafe {
        prepare_supervisor();
        // A recovered probe changes live trap state. Finish probes before
        // staging the status and PC consumed by the final mret.
        #[cfg(target_pointer_width = "32")]
        if crate::csr::read::<machine::StatusHigh>().is_ok() {
            machine::StatusHigh::modify(|_, high| {
                high.write::<machine::MPV>(machine::Virtualization::Off);
            });
        }
        machine::Status::modify(|_, status| set_return_mode(status, next_mode));
        machine::ExceptionPc::write(start_addr.into());
    }
}
