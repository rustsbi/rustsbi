//! Transactional per-hart trap initialization.
//!
//! The private lifecycle is `Uninitialized → Ready → Armed`:
//! [`init`] publishes `Ready` transactionally with the final normal `mtvec`
//! write as its commit point; the never-returning
//! [`finish_boot`](crate::boot::finish_boot) later establishes `Armed` with
//! the clean stack top in `mscratch`.

use alloc::boxed::Box;
use core::arch::asm;
use core::fmt;
use core::sync::atomic::{AtomicU8, Ordering};

use riscv::register::medeleg;
use rustsbi::RustSBI;
use spin::Once;

use super::ValueKind;
use super::entry::trap_entry;
use crate::hart::HartId;

/// Private lifecycle phases.
const PHASE_UNINITIALIZED: u8 = 0;
const PHASE_INITIALIZING: u8 = 1;
const PHASE_READY: u8 = 2;
const PHASE_ARMED: u8 = 3;

/// Marks `hart`'s lifecycle phase Armed; called only by the divergent
/// `finish_boot` on the current hart.
pub(crate) fn mark_armed(hart: HartId) {
    let state = hart_states()
        .get(hart.index())
        .expect("BUG: hart index is outside the boot topology");
    state.phase.store(PHASE_ARMED, Ordering::Release);
}

/// An error from [`init`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InitError {
    /// This hart's trap state is already initialized.
    AlreadyInitialized,
    /// `mhartid` is not an enabled hart in the boot topology.
    InvalidHartId,
    /// The current hart's timer could not be initialized.
    Timer(crate::timer::Error),
    /// The current hart's firmware IPI source could not be initialized.
    Ipi(crate::ipi::Error),
}

impl fmt::Display for InitError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AlreadyInitialized => {
                formatter.write_str("trap state already initialized on this hart")
            }
            Self::InvalidHartId => formatter.write_str("hart ID is not in the boot topology"),
            Self::Timer(error) => write!(formatter, "timer initialization failed: {error}"),
            Self::Ipi(error) => write!(formatter, "IPI initialization failed: {error}"),
        }
    }
}

/// The per-hart Runtime trap state: lifecycle phase, erased policy, and the
/// architectural Sstc capability, shared only under `Sync`. Platform services
/// live in their own subsystem modules rather than in CPU-local state.
struct HartState {
    phase: AtomicU8,
    policy: Once<&'static (dyn RustSBI + Sync)>,
}

static HARTS: Once<Box<[HartState]>> = Once::new();

fn hart_states() -> &'static [HartState] {
    HARTS.call_once(|| {
        HartId::all()
            .map(|_| HartState {
                phase: AtomicU8::new(PHASE_UNINITIALIZED),
                policy: Once::new(),
            })
            .collect()
    })
}

/// Returns `hart`'s published policy.
///
/// # Panics
///
/// Panics when `hart` was never initialized; dispatch only runs after
/// `init` published `Ready`.
pub(crate) fn policy(hart: HartId) -> &'static (dyn RustSBI + Sync) {
    let state = hart_states()
        .get(hart.index())
        .expect("BUG: hart index is outside the boot topology");
    if state.phase.load(Ordering::Acquire) < PHASE_READY {
        unreachable!("BUG: trap dispatch before trap::init published Ready");
    }
    *state
        .policy
        .get()
        .expect("initialized phase implies a policy")
}

/// Initializes trap handling on the current hart and stores the policy in this
/// hart's private slot. Platform services are published by
/// their own subsystem modules before this function is called.
/// Machine interrupts stay disabled throughout; `mscratch` keeps the zero boot
/// sentinel, so an unexpected trap before `finish_boot` fail-stops.
pub fn init<P>(policy: &'static P) -> Result<(), InitError>
where
    P: RustSBI + Sync + 'static,
{
    // Reject unknown harts before indexing per-hart state or writing CSRs.
    let hart = HartId::current()
        .map_err(|_| InitError::InvalidHartId)?
        .index();
    let Some(state) = hart_states().get(hart) else {
        return Err(InitError::InvalidHartId);
    };

    // Reserve before touching devices so repeated initialization cannot replace active state.
    if state
        .phase
        .compare_exchange(
            PHASE_UNINITIALIZED,
            PHASE_INITIALIZING,
            Ordering::AcqRel,
            Ordering::Acquire,
        )
        .is_err()
    {
        return Err(InitError::AlreadyInitialized);
    }

    // Keep policy unpublished until both IPI and timer initialization succeed.
    let ipi_result = match crate::ipi::Ipi::current() {
        Ok(ipi) => ipi.initialize(),
        Err(crate::ipi::Error::Unavailable) => Ok(()),
        Err(error) => Err(error),
    };
    if let Err(error) = ipi_result {
        state.phase.store(PHASE_UNINITIALIZED, Ordering::Release);
        return Err(InitError::Ipi(error));
    }
    if let Err(error) =
        crate::timer::Timer::current().and_then(|timer| timer.initialize_current_hart())
    {
        state.phase.store(PHASE_UNINITIALIZED, Ordering::Release);
        return Err(InitError::Timer(error));
    }
    state.policy.call_once(|| policy as &(dyn RustSBI + Sync));

    // Delegation is WARL; dispatch handles or software-redirects exceptions
    // that hardware retains in M-mode.
    // SAFETY:
    // 1. Runtime initializes the current hart in M-mode.
    // 2. These writes establish its fixed delegation and counter-access policy.
    unsafe {
        asm!("csrw mideleg,    {}", in(reg) !0);
        asm!("csrw medeleg,    {}", in(reg) !0);
        asm!("csrw mcounteren, {}", in(reg) !0);
        asm!("csrw scounteren, {}", in(reg) !0);
        medeleg::clear_supervisor_env_call();
        medeleg::clear_load_misaligned();
        medeleg::clear_store_misaligned();
        medeleg::clear_illegal_instruction();
        // Count access faults before redirecting them to the supervisor.
        medeleg::clear_load_fault();
        medeleg::clear_store_fault();
    }

    state.phase.store(PHASE_READY, Ordering::Release);
    // Retain the early fail-stop vector until devices and policy are ready.
    // SAFETY:
    // 1. Runtime executes in M-mode and has published this hart's policy.
    // 2. The assembly entry is aligned for an M-mode direct vector.
    unsafe {
        riscv::register::mtvec::write(riscv::register::mtvec::Mtvec::new(
            trap_entry as *const () as _,
            riscv::register::mtvec::TrapMode::Direct,
        ))
    };
    Ok(())
}

/// An error returned when a platform dispatcher declines an access.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AccessError;

/// A platform service that completes S-mode load/store accesses the hardware
/// refused.
///
/// Runtime keeps the instruction semantics, fetching and decoding the trapped
/// instruction, extending the loaded value, and advancing `mepc`. The service
/// only performs the access itself, receiving the `mtval`-reported faulting
/// address, the [`ValueKind`] (width and signedness), and the value for stores.
///
/// Declining (`Err`) leaves the original fault to be redirected to
/// the supervisor unchanged.
pub trait AccessDispatcher: Sync {
    /// Completes a load of `kind` at `addr`, returning the raw unsigned value,
    /// or `Err` if the access cannot be completed.
    fn load(&self, addr: usize, kind: ValueKind) -> Result<usize, AccessError>;

    /// Completes a store of the `kind.width()` low-order bytes of `value` at
    /// `addr`, or returns `Err` if the access cannot be completed.
    fn store(&self, addr: usize, kind: ValueKind, value: usize) -> Result<(), AccessError>;
}

/// The erased access-fault service: one global dispatcher shared by every
/// hart, published once during boot.
static ACCESS_DISPATCHER: Once<&'static dyn AccessDispatcher> = Once::new();

/// Publishes the platform's access-fault dispatcher once during boot.
///
/// This uses the same `Once`-backed erased-reference pattern as the SBI
/// policy stored by [`init`] — though that policy is per-hart while this
/// dispatcher is global — and trap dispatch reads it directly. Later calls
/// are ignored.
pub fn install_access_dispatcher<D>(dispatcher: &'static D)
where
    D: AccessDispatcher + 'static,
{
    ACCESS_DISPATCHER.call_once(|| dispatcher as &dyn AccessDispatcher);
}

/// Returns the published access-fault dispatcher, or `None` when the
/// platform installed none.
pub(crate) fn access_dispatcher() -> Option<&'static dyn AccessDispatcher> {
    ACCESS_DISPATCHER.get().copied()
}
