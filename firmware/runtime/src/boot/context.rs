//! Trap installation and next-stage architectural context.
#[cfg(target_pointer_width = "32")]
use crate::fault_recovery::csrr;
use crate::trap::entry;
use crate::{
    Error, Result,
    hart::{Hart, HartLocal},
    irq,
};
use csr::{
    Write, machine, supervisor,
    values::{
        counter::CounterSet,
        trap::{Exception, ExceptionSet, InterruptSet},
        vector::{TargetVectorMode, VectorBase, VectorMode},
    },
};

static SETUP: HartLocal<spin::Mutex<SetupPhase>> = HartLocal::uninit();

enum SetupPhase {
    Initializing,
    Configured,
    Entered,
    Failed,
}

impl SetupPhase {
    fn require_initializing(&self) -> Result<()> {
        match self {
            Self::Initializing => Ok(()),
            Self::Configured => Err(Error::AlreadyInitialized),
            Self::Entered | Self::Failed => Err(Error::InvalidState),
        }
    }
}

/// Installs machine exception handling before platform feature probes.
///
/// # Safety
///
/// Run in M-mode with a valid Runtime stack, local machine interrupts disabled,
/// and `mscratch` set to zero.
#[doc(hidden)]
pub unsafe extern "C" fn prepare_traps() {
    // SAFETY: Startup owns this hart's trap state before entering policy.
    unsafe {
        install(entry::trap_entry);
        machine::Status::modify(|_, status| {
            status.write_field::<machine::MPRV>(machine::AccessPrivilege::Current);
            #[cfg(target_pointer_width = "64")]
            status.write_field::<machine::MDT>(machine::DoubleTrapState::Clear);
        });
    }
    // Install the basic vector before probing so an absent mstatush can recover.
    #[cfg(target_pointer_width = "32")]
    let high = csrr!(machine::StatusHigh).ok();
    #[cfg(target_pointer_width = "32")]
    if high.is_some() {
        // SAFETY: The probe confirmed mstatush; startup owns trap setup.
        unsafe {
            machine::StatusHigh::modify(|_, status| {
                status.write_field::<machine::MDT>(machine::DoubleTrapState::Clear);
            });
            install(entry::trap_entry_high);
        }
    }
    if machine::Isa::read().has_extension('H') {
        // SAFETY: H is present and startup owns trap setup.
        unsafe {
            install(entry::trap_entry_hypervisor);
        }
    }
    SETUP
        .try_init_current(spin::Mutex::new(SetupPhase::Initializing))
        .expect("Runtime startup must run once per hart");
}

/// Checks the Direct trap entry installed by controlled Runtime boot on this hart.
pub(crate) fn traps_ready() -> bool {
    let vector = machine::TrapVector::read();
    let base = usize::from(vector.read_field::<machine::BASE>().address());
    let entry_matches = base == entry::trap_entry as *const () as usize
        || base == entry::trap_entry_hypervisor as *const () as usize;
    #[cfg(target_pointer_width = "32")]
    let entry_matches = entry_matches || base == entry::trap_entry_high as *const () as usize;
    vector.read_field::<machine::MODE>() == VectorMode::Direct
        && entry_matches
        && machine::Status::read().read_field::<machine::MPRV>() == machine::AccessPrivilege::Current
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
    let mut vector = machine::TrapVector::<Write>::default();
    vector.write_field::<machine::BASE>(base);
    vector.write_field::<machine::MODE>(TargetVectorMode::Direct);
    // SAFETY: The caller supplies a Runtime trap entry.
    unsafe { machine::TrapVector::write(vector) }
}

/// Configures the initial supervisor environment, delegation, and firmware features.
///
/// Runtime excludes local interrupts and requires its initial startup phase.
/// Configuration succeeds once; repeat calls leave hardware and locks unchanged.
/// A failed attempt prevents handoff and cannot be retried on this hart.
///
/// # Errors
/// Returns [`Error::NotReady`] without Runtime entry or with active `MPRV`.
/// Returns [`Error::AlreadyInitialized`] after successful configuration, or
/// [`Error::InvalidState`] after handoff or a failed configuration attempt.
pub(crate) fn configure_next_stage(hart: &Hart) -> Result<()> {
    with_setup(|phase| {
        phase.require_initializing()?;
        // A partial failure must not permit handoff or another initialization.
        *phase = SetupPhase::Failed;
        // SAFETY: Runtime established initial entry, excludes local IRQs, and
        // has not handed off this hart. No lower-mode execution is active.
        unsafe {
            hart.configure_page_tables()?;
            hart.configure_cache_blocks()?;
            hart.configure_seed_access()?;
            hart.initialize_state_enable()?;
            let inhibit = crate::pmu::configure_initial()?;
            crate::pmu::Pmu::init_current(inhibit)?;
            let irq = crate::irq::LocalIrqDisabled::assume_disabled();
            crate::ipi::get().ok_or(Error::NotReady)?.init_current(&irq)?;
            crate::timer::get().ok_or(Error::NotReady)?.init_current(&irq)?;
            if hart.supports_privilege(machine::TargetPrivilege::Supervisor) {
                machine::InterruptDelegation::write(usize::MAX.into());
                machine::ExceptionDelegation::write(ExceptionSet::except(&[
                    Exception::SupervisorEcall,
                    Exception::LoadMisaligned,
                    Exception::StoreMisaligned,
                    Exception::IllegalInstruction,
                    Exception::LoadAccess,
                    Exception::StoreAccess,
                ]));
                supervisor::CounterEnable::write(CounterSet::all());
            }
            if hart.supports_privilege(machine::TargetPrivilege::Supervisor)
                || hart.supports_privilege(machine::TargetPrivilege::User)
            {
                machine::CounterEnable::write(CounterSet::all());
            }
            crate::fwfeatures::FwFeatures::init_current(hart)?;
        }
        *phase = SetupPhase::Configured;
        Ok(())
    })
}

fn with_setup<T>(operation: impl FnOnce(&mut SetupPhase) -> Result<T>) -> Result<T> {
    if !traps_ready() || usize::from(machine::Scratch::read()) != 0 {
        return Err(Error::NotReady);
    }
    irq::with_local_disabled(|_| {
        let setup = SETUP.try_get_current().ok_or(Error::NotReady)?;
        let mut phase = setup.try_lock().ok_or(Error::Busy)?;
        operation(&mut phase)
    })
}

pub(super) fn complete_setup() {
    let mut phase = SETUP.get_current().lock();
    assert!(
        matches!(*phase, SetupPhase::Configured | SetupPhase::Entered),
        "cannot hand off a hart without successful Runtime configuration"
    );
    *phase = SetupPhase::Entered;
}

pub(crate) fn set_return_mode(status: &mut machine::Status<Write>, mode: machine::TargetPrivilege) {
    status.write_field::<machine::MPP>(mode);
    status.write_field::<machine::MPIE>(machine::Interrupt::Enabled);
    status.write_field::<machine::SIE>(machine::Interrupt::Disabled);
    #[cfg(target_pointer_width = "64")]
    status.write_field::<machine::MPV>(machine::Virtualization::Off);
}

/// Prepares translation and interrupt state for a new supervisor stage.
///
/// # Safety
///
/// Run in M-mode with interrupts disabled, `MPRV` clear, and Runtime trap entry
/// installed.
/// The outgoing supervisor address space must no longer be needed.
pub(crate) unsafe fn prepare_supervisor() {
    // SAFETY: The caller is replacing the outgoing supervisor context;
    // clearing its translation and interrupt enables cannot invalidate M-mode state.
    unsafe {
        if Hart::current()
            .expect("handoff hart")
            .supports_privilege(machine::TargetPrivilege::Supervisor)
        {
            supervisor::InterruptEnable::write(InterruptSet::empty());
            supervisor::AddressTranslation::write(supervisor::AddressSpace::bare());
        }
    }
    crate::fwfeatures::FwFeatures::restore_current()
        .expect("could not restore committed firmware features before handoff");
    // SAFETY: This function's caller keeps MIE clear throughout the handoff.
    let irq = unsafe { crate::irq::LocalIrqDisabled::assume_disabled() };
    crate::ipi::get()
        .expect("IPI service not published")
        .prepare_wakeup(&irq);
}

/// Programs the initial boot handoff in the current hart's CSRs.
// Trap handoffs instead update the saved frame restored by entry.
///
/// # Safety
///
/// M-mode on the current hart, interrupts disabled, no live lower context.
pub(super) unsafe fn stage_next_mode(start_addr: usize, next_mode: machine::TargetPrivilege) {
    // SAFETY: The boot caller owns the outgoing trap state.
    unsafe {
        prepare_supervisor();
    }
    // A recovered probe changes live trap state. Finish probes before
    // staging the status and PC consumed by the final mret.
    #[cfg(target_pointer_width = "32")]
    if csrr!(machine::StatusHigh).is_ok() {
        // SAFETY: The probe succeeded; boot owns the outgoing trap state.
        unsafe {
            machine::StatusHigh::modify(|_, high| {
                high.write_field::<machine::MPV>(machine::Virtualization::Off);
            });
        }
    }
    // SAFETY: Boot owns the outgoing trap state after all recovered probes.
    unsafe {
        machine::Status::modify(|_, status| set_return_mode(status, next_mode));
        machine::ExceptionPc::write(start_addr.into());
    }
}
