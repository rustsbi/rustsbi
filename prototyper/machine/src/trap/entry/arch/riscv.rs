//! RISC-V trap-vector installation, state lookup, and terminal transitions.

use crate::CounterError;
use crate::boot::{NextMode, NextStage};
use crate::config::HART_CAPACITY;
use crate::trap::Trap;
use crate::trap::dispatch::dispatch;
use crate::trap::frame::{self, Frame};

use super::super::{
    CURRENT_FRAME_OFFSET, DEPTH_OFFSET, FLAG_HYPERVISOR, FLAGS_OFFSET, HART_TRAP_STATES,
    HartTrapState, SAVED_SP_OFFSET, SAVED_T0_OFFSET, SAVED_T1_OFFSET, SAVED_T2_OFFSET,
    SAVED_T3_OFFSET, STACK_BOTTOM_OFFSET, STACK_TOP_OFFSET, TRAP_STATE_READY, TrapStateError,
    abort,
};

pub(crate) fn prepare_hypervisor_metadata() -> Result<(), TrapStateError> {
    let state = current_state().ok_or(TrapStateError::InvalidIndex)?;
    if crate::csr::probe_hypervisor_metadata().map_err(|_| TrapStateError::FeatureProbe)? {
        state.enable_hypervisor_metadata();
    }
    Ok(())
}

pub(crate) fn current_index() -> Option<usize> {
    let state = current_state()?;
    state.is_ready().then(|| state.index())
}

pub(crate) fn activate(index: usize) -> Result<(), TrapStateError> {
    let state = HART_TRAP_STATES
        .get(index)
        .ok_or(TrapStateError::InvalidIndex)?;
    if !state.is_ready() {
        return Err(TrapStateError::InvalidIndex);
    }
    let state = state as *const HartTrapState as usize;
    let entry = __rustsbi_prototyper_trap_entry as *const () as usize;
    // SAFETY: the selected state was fully initialized and Acquire-observed;
    // the direct-mode entry symbol is aligned; this hart has not enabled any
    // machine interrupt and owns its trap-local activation transition.
    unsafe {
        core::arch::asm!(
            "csrw mscratch, {state}",
            "csrw mtvec, {entry}",
            state = in(reg) state,
            entry = in(reg) entry,
            options(nostack, preserves_flags),
        );
    }
    Ok(())
}

pub(crate) fn prepare_counters(index: usize, mode: NextMode) -> Result<(), CounterError> {
    let state = HART_TRAP_STATES
        .get(index)
        .ok_or(CounterError::MechanismFailure)?;
    if !state.is_ready() {
        return Err(CounterError::MechanismFailure);
    }
    // SAFETY: initialization writes the capability before Release publication
    // and never changes it. Its hart-local slot serializes preparation.
    unsafe {
        (&*state.counters.get())
            .as_ref()
            .map_or(Ok(()), |counters| {
                counters.prepare_current()?;
                crate::csr::prepare_counter_access(mode, counters)
                    .map_err(|_| CounterError::MechanismFailure)
            })
    }
}

/// Abandons the stopped supervisor context and rejoins the machine warm loop.
pub(crate) fn park_current_hart() -> ! {
    let Some(state) = current_state() else {
        abort();
    };
    let index = state.index();
    let Some(stack) = crate::startup::hart_stack_top(index) else {
        abort();
    };
    let hart_id = current_hart_id();
    disable_interrupts_and_arm_double_trap();
    if state.abandon_current_frame().is_err() {
        abort();
    }
    // SAFETY: the HSM gate published Stopped and drained accepted work. The
    // selected permanent stack is disjoint, and the old frame is unreachable.
    unsafe { crate::startup::enter_warm_loop(hart_id, index, stack) }
}

/// Abandons a suspended frame and enters its validated resume stage.
pub(crate) fn enter_resumed_stage(next_stage: NextStage) -> ! {
    let Some(state) = current_state() else {
        abort();
    };
    let hart_id = current_hart_id();
    disable_interrupts_and_arm_double_trap();
    if state.abandon_current_frame().is_err() {
        abort();
    }
    let Some(runtime) = crate::hart::runtime::runtime() else {
        abort();
    };
    if runtime.prepare_current_hart().is_err() {
        abort();
    }
    crate::boot::enter(next_stage, hart_id, None)
}

pub(crate) fn current_state() -> Option<&'static HartTrapState> {
    let address: usize;
    // SAFETY: reading mscratch has no memory effect. Rust is entered only while
    // it contains the installed trap-state pointer.
    unsafe {
        core::arch::asm!(
            "csrr {address}, mscratch",
            address = out(reg) address,
            options(nomem, nostack, preserves_flags),
        );
    }
    let base = HART_TRAP_STATES.as_ptr() as usize;
    let stride = core::mem::size_of::<HartTrapState>();
    let bytes = stride.checked_mul(HART_CAPACITY)?;
    let end = base.checked_add(bytes)?;
    if address < base || address >= end || !(address - base).is_multiple_of(stride) {
        return None;
    }
    let index = (address - base) / stride;
    let state = HART_TRAP_STATES.get(index)?;
    state.is_ready().then_some(state)
}

pub(crate) fn restore(trap: Trap<'_>) -> ! {
    if crate::power::is_terminal() {
        crate::power::halt();
    }
    let frame = trap.frame as *mut Frame;
    let state = trap.state as *const HartTrapState;
    // SAFETY: consuming Trap ended the sole Rust frame authority. The checked
    // frame/state pair remains published until assembly removes the relation.
    unsafe { __rustsbi_prototyper_trap_restore(frame, state.cast()) }
}

fn current_hart_id() -> usize {
    let hart_id;
    // SAFETY: mhartid is a mandatory read-only machine CSR.
    unsafe {
        core::arch::asm!("csrr {hart_id}, mhartid", hart_id = out(reg) hart_id, options(nomem, nostack))
    };
    hart_id
}

#[cfg(target_pointer_width = "64")]
fn disable_interrupts_and_arm_double_trap() {
    const MIE: usize = 1 << 3;
    const MDT: usize = 1 << 42;
    // SAFETY: closes interrupt entry before removing frame publication and
    // restores the trap-entry double-trap guard.
    unsafe {
        core::arch::asm!(
            "csrc mstatus, {mie}",
            "csrw mie, zero",
            "csrs mstatus, {mdt}",
            mie = in(reg) MIE,
            mdt = in(reg) MDT,
            options(nostack),
        )
    }
}

#[cfg(target_pointer_width = "32")]
fn disable_interrupts_and_arm_double_trap() {
    const MIE: usize = 1 << 3;
    const MDT: usize = 1 << 10;
    // SAFETY: same transition as RV64; MDT resides in mstatush on RV32.
    unsafe {
        core::arch::asm!(
            "csrc mstatus, {mie}",
            "csrw mie, zero",
            "csrs mstatush, {mdt}",
            mie = in(reg) MIE,
            mdt = in(reg) MDT,
            options(nostack),
        )
    }
}

unsafe extern "C" {
    fn __rustsbi_prototyper_trap_entry();
    fn __rustsbi_prototyper_trap_restore(frame: *mut Frame, state: *const ()) -> !;
}

core::arch::global_asm!(
    include_str!("../../entry.S"),
    word_size = const core::mem::size_of::<usize>(),
    frame_size = const frame::FRAME_SIZE,
    frame_align_mask = const !(frame::FRAME_ALIGN - 1),
    trap_state_ready = const TRAP_STATE_READY,
    flag_hypervisor = const FLAG_HYPERVISOR,
    stack_bottom = const STACK_BOTTOM_OFFSET,
    stack_top = const STACK_TOP_OFFSET,
    current_frame = const CURRENT_FRAME_OFFSET,
    depth = const DEPTH_OFFSET,
    flags = const FLAGS_OFFSET,
    saved_sp = const SAVED_SP_OFFSET,
    saved_t0 = const SAVED_T0_OFFSET,
    saved_t1 = const SAVED_T1_OFFSET,
    saved_t2 = const SAVED_T2_OFFSET,
    saved_t3 = const SAVED_T3_OFFSET,
    x0 = const frame::X0_OFFSET,
    x1 = const frame::X1_OFFSET,
    x2 = const frame::X2_OFFSET,
    x3 = const frame::X3_OFFSET,
    x4 = const frame::X4_OFFSET,
    x5 = const frame::X5_OFFSET,
    x6 = const frame::X6_OFFSET,
    x7 = const frame::X7_OFFSET,
    x8 = const frame::X8_OFFSET,
    x9 = const frame::X9_OFFSET,
    x10 = const frame::X10_OFFSET,
    x11 = const frame::X11_OFFSET,
    x12 = const frame::X12_OFFSET,
    x13 = const frame::X13_OFFSET,
    x14 = const frame::X14_OFFSET,
    x15 = const frame::X15_OFFSET,
    x16 = const frame::X16_OFFSET,
    x17 = const frame::X17_OFFSET,
    x18 = const frame::X18_OFFSET,
    x19 = const frame::X19_OFFSET,
    x20 = const frame::X20_OFFSET,
    x21 = const frame::X21_OFFSET,
    x22 = const frame::X22_OFFSET,
    x23 = const frame::X23_OFFSET,
    x24 = const frame::X24_OFFSET,
    x25 = const frame::X25_OFFSET,
    x26 = const frame::X26_OFFSET,
    x27 = const frame::X27_OFFSET,
    x28 = const frame::X28_OFFSET,
    x29 = const frame::X29_OFFSET,
    x30 = const frame::X30_OFFSET,
    x31 = const frame::X31_OFFSET,
    mepc = const frame::MEPC_OFFSET,
    mstatus = const frame::MSTATUS_OFFSET,
    mstatus_high = const frame::MSTATUS_HIGH_OFFSET,
    cause = const frame::CAUSE_OFFSET,
    tval = const frame::TVAL_OFFSET,
    tval2 = const frame::TVAL2_OFFSET,
    tinst = const frame::TINST_OFFSET,
    gva = const frame::GVA_OFFSET,
    previous = const frame::PREVIOUS_OFFSET,
    dispatch = sym dispatch,
);
