//! Transactional per-hart trap initialization.
//!
//! The private lifecycle is `Uninitialized → Ready → Armed`:
//! [`init`] publishes `Ready` transactionally with the final normal `mtvec`
//! write as its commit point; the never-returning
//! [`finish_boot`](crate::boot::finish_boot) later establishes `Armed` with
//! the clean stack top in `mscratch`.

use core::arch::asm;
use core::fmt;
use core::sync::atomic::{AtomicBool, AtomicU8, Ordering};

use riscv::register::medeleg;
use rustsbi::RustSBI;
use spin::Once;

use super::entry::trap_entry;
use crate::cfg::NUM_HART_MAX;
use crate::hart::{HartId, current_hart};

/// Private lifecycle phases.
const PHASE_UNINITIALIZED: u8 = 0;
const PHASE_INITIALIZING: u8 = 1;
const PHASE_READY: u8 = 2;
const PHASE_ARMED: u8 = 3;

/// Marks `hart`'s lifecycle phase Armed; called only by the divergent
/// `finish_boot` on the current hart.
pub(crate) fn mark_armed(hart: usize) {
    let state = HARTS
        .get(hart)
        .expect("BUG: hart ID exceeds the configured limit");
    state.phase.store(PHASE_ARMED, Ordering::Release);
}

/// An error from [`init`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InitError {
    /// This hart's trap state is already initialized.
    AlreadyInitialized,
    /// `mhartid` is beyond the Runtime's configured hart capacity.
    InvalidHartId,
}

impl fmt::Display for InitError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::AlreadyInitialized => "trap state already initialized on this hart",
            Self::InvalidHartId => "hart ID beyond the configured capacity",
        })
    }
}

/// The per-hart Runtime trap state: lifecycle phase, erased policy, and the
/// architectural Sstc capability, shared only under `Sync`. Platform services
/// live in their own subsystem modules rather than in CPU-local state.
struct HartState {
    phase: AtomicU8,
    policy: Once<&'static (dyn RustSBI + Sync)>,
    has_sstc: AtomicBool,
}

static HARTS: [HartState; NUM_HART_MAX] = [const {
    HartState {
        phase: AtomicU8::new(PHASE_UNINITIALIZED),
        policy: Once::new(),
        has_sstc: AtomicBool::new(false),
    }
}; NUM_HART_MAX];

/// Returns the current hart's published policy.
///
/// # Panics
///
/// Panics when this hart was never initialized; dispatch only runs after
/// `init` published `Ready`.
pub(crate) fn policy() -> &'static (dyn RustSBI + Sync) {
    let hart = current_hart().as_usize();
    let state = HARTS
        .get(hart)
        .expect("BUG: hart ID exceeds the configured limit");
    if state.phase.load(Ordering::Acquire) < PHASE_READY {
        unreachable!("BUG: trap dispatch before trap::init published Ready");
    }
    *state
        .policy
        .get()
        .expect("initialized phase implies a policy")
}

/// Returns whether the current hart implements the Sstc `stimecmp` CSR.
pub(crate) fn has_sstc() -> bool {
    let hart = current_hart().as_usize();
    let state = HARTS
        .get(hart)
        .expect("BUG: hart ID exceeds the configured limit");
    state.has_sstc.load(Ordering::Acquire)
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
    // 1. Validate the hart before any address arithmetic or CSR writes.
    let hart = HartId::current()
        .map_err(|_| InitError::InvalidHartId)?
        .as_usize();
    let Some(state) = HARTS.get(hart) else {
        return Err(InitError::InvalidHartId);
    };

    // 2. Reserve the slot transactionally.
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

    // 3. Initialize the handler state before publishing Ready.
    state.policy.call_once(|| policy as &(dyn RustSBI + Sync));

    // Sstc is an architectural capability, so probe its CSR here instead of
    // asking the platform adapter to report it on every timer trap. The
    // guarded read safely turns an absent CSR into `false`.
    let has_sstc = crate::csr::has_stimecmp();
    state.has_sstc.store(has_sstc, Ordering::Release);

    // 4. Fixed delegation and counter policy. The
    //    delegation CSRs are WARL; retained exceptions are handled below
    //    or software-redirected by the dispatch.
    // SAFETY: M-mode init on the current hart; the written values are the
    // firmware's fixed delegation policy.
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
    // 5. Commit Ready by installing the final normal vector last: before
    //    this write a trap reaches the early fail-stop vector.
    // SAFETY: the Runtime-owned entry is a valid, aligned M-mode direct
    // target.
    unsafe {
        riscv::register::mtvec::write(riscv::register::mtvec::Mtvec::new(
            trap_entry as *const () as _,
            riscv::register::mtvec::TrapMode::Direct,
        ))
    };
    Ok(())
}

/// Misaligned load/store exception `medeleg` bits (causes 4 and 6).
const MIS_DELEG: usize = (1 << 4) | (1 << 6);

/// Reads whether misaligned load/store exceptions are delegated to S-mode.
///
/// Narrow fact backing the FWFT extension's `MISALIGNED_EXC_DELEG` feature;
/// no arbitrary delegation mask is exposed.
pub fn misaligned_delegated() -> bool {
    (riscv::register::medeleg::read().bits() & MIS_DELEG) != 0
}

/// Sets or clears the misaligned load/store exception delegation,
/// preserving every other `medeleg` bit.
///
/// Narrow operation backing the FWFT extension's `MISALIGNED_EXC_DELEG`
/// feature; `medeleg` is trap-sensitive CSR, so policy reaches it only
/// through this operation.
pub fn set_misaligned_delegation(enabled: bool) {
    let current = riscv::register::medeleg::read().bits();
    let next = if enabled {
        current | MIS_DELEG
    } else {
        current & !MIS_DELEG
    };
    // SAFETY: M-mode write on the current hart; `next` preserves every
    // `medeleg` bit except the two misaligned exception bits.
    unsafe {
        riscv::register::medeleg::write(riscv::register::medeleg::Medeleg::from_bits(next));
    }
}
