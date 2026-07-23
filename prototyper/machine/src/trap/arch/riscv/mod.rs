//! RISC-V trap-vector installation and short architectural leaves.

use crate::boot::{NextMode, NextStage};
use crate::trap::Trap;
use crate::trap::dispatch::dispatch;
use crate::trap::expected::{ExpectedResult, probe_csr};
use crate::trap::frame::{self, Frame};
use crate::trap::{features, stack};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TrapInstallError {
    AlreadyInstalled,
    InvalidIndex,
    InvalidLayout,
    FeatureProbe,
}

pub(crate) fn install(
    hart_count: usize,
    handler: &'static dyn crate::SbiHandler,
) -> Result<(), TrapInstallError> {
    crate::trap::dispatch::install(handler).map_err(|_| TrapInstallError::AlreadyInstalled)?;
    stack::admit(hart_count).map_err(|error| match error {
        stack::StackError::AlreadyAdmitted => TrapInstallError::AlreadyInstalled,
        stack::StackError::InvalidIndex => TrapInstallError::InvalidIndex,
        stack::StackError::InvalidLayout => TrapInstallError::InvalidLayout,
    })
}

pub(crate) fn activate(index: usize) -> Result<(), TrapInstallError> {
    let stack_top = stack::top(index).map_err(|_| TrapInstallError::InvalidIndex)?;
    let entry = __rustsbi_prototyper_trap_entry as *const () as usize;
    // SAFETY: stack admission was Release-published before this Acquire lookup.
    // The direct-mode vector is aligned and this hart owns local activation.
    unsafe {
        core::arch::asm!(
            "csrw mscratch, {stack_top}",
            "csrw mtvec, {entry}",
            stack_top = in(reg) stack_top,
            entry = in(reg) entry,
            options(nostack, preserves_flags),
        );
    }
    Ok(())
}

pub(crate) fn current_index() -> Option<usize> {
    let stack_top: usize;
    // SAFETY: lower-mode and trap Rust execution keep mscratch equal to the
    // admitted current hart's fixed trap-stack top.
    unsafe {
        core::arch::asm!(
            "csrr {stack_top}, mscratch",
            stack_top = out(reg) stack_top,
            options(nomem, nostack, preserves_flags),
        );
    }
    stack::index_for_top(stack_top)
}

pub(crate) fn prepare_hypervisor_metadata() -> Result<(), TrapInstallError> {
    let index = current_index().ok_or(TrapInstallError::InvalidIndex)?;
    if probe_hypervisor_metadata().map_err(|_| TrapInstallError::FeatureProbe)?
        && (!features::enable_hypervisor_metadata(index)
            || !crate::trap::expected::enable_hypervisor_metadata(index))
    {
        return Err(TrapInstallError::InvalidIndex);
    }
    Ok(())
}

const MISA: u16 = 0x301;
const MTINST: u16 = 0x34a;
const MTVAL2: u16 = 0x34b;
const HSTATUS: u16 = 0x600;
const MISA_H: usize = 1 << 7;

fn probe_hypervisor_metadata() -> Result<bool, ()> {
    let Some(misa) = probe_optional::<MISA>()? else {
        return Ok(false);
    };
    if misa & MISA_H == 0 {
        return Ok(false);
    }
    if probe_optional::<MTINST>()?.is_some()
        && probe_optional::<MTVAL2>()?.is_some()
        && probe_optional::<HSTATUS>()?.is_some()
    {
        Ok(true)
    } else {
        Err(())
    }
}

fn probe_optional<const CSR: u16>() -> Result<Option<usize>, ()> {
    // SAFETY: each instantiation names a fixed CSR owned by trap setup.
    match unsafe { probe_csr::<CSR>() } {
        ExpectedResult::Value(value) => Ok(Some(value)),
        ExpectedResult::Fault(fault) if fault.cause == 2 => Ok(None),
        ExpectedResult::Fault(_) | ExpectedResult::Busy | ExpectedResult::Unavailable => Err(()),
    }
}

#[crate::mtest]
fn machine_identity_matches_xlen() {
    let expected_mxl = if usize::BITS == 32 { 1 } else { 2 };
    let misa = probe_optional::<MISA>()
        .expect("misa probe must complete")
        .expect("misa must be implemented");
    assert_eq!(misa >> (usize::BITS - 2), expected_mxl);
}

#[cfg(test)]
#[test]
fn hypervisor_detection_uses_the_standard_misa_bit() {
    assert_eq!(MISA_H, 1 << 7);
}

pub(crate) fn hypervisor_available(index: usize) -> bool {
    features::hypervisor_metadata_available(index)
}

pub(crate) fn prepare_timer() -> Result<(), crate::TimerError> {
    crate::timer::prepare_current_hart()
}

pub(crate) fn prepare_counters(mode: NextMode) -> Result<(), crate::CounterError> {
    crate::pmu::prepare_current(mode)
}

/// Abandons the stopped supervisor frame and rejoins the warm loop.
pub(crate) fn park_current_hart() -> ! {
    let Some(index) = current_index() else {
        crate::trap::abort();
    };
    let Some(runtime_stack_top) = crate::startup::hart_stack_top(index) else {
        crate::trap::abort();
    };
    let hart_id = current_hart_id();
    disable_interrupts_and_arm_double_trap();
    // SAFETY: HSM published Stopped and drained accepted work. This
    // non-returning transition makes the abandoned trap frame unreachable.
    unsafe { crate::startup::enter_warm_loop(hart_id, index, runtime_stack_top) }
}

/// Abandons a suspended frame and enters its validated resume stage.
pub(crate) fn enter_resumed_stage(next_stage: NextStage) -> ! {
    let hart_id = current_hart_id();
    disable_interrupts_and_arm_double_trap();
    let Some(admission) = crate::hart::protocol::installed() else {
        crate::trap::abort();
    };
    if admission.prepare_current_hart().is_err() {
        crate::trap::abort();
    }
    crate::boot::enter(next_stage, hart_id, None)
}

pub(crate) fn capture_hypervisor_metadata(frame: &mut Frame) {
    let value2: usize;
    let instruction: usize;
    // SAFETY: current-hart preparation proved H trap metadata CSR support
    // before publishing the feature fact. Trap CSRs still describe this frame.
    unsafe {
        core::arch::asm!(
            "csrr {value2}, 0x34b",
            "csrr {instruction}, 0x34a",
            value2 = out(reg) value2,
            instruction = out(reg) instruction,
            options(nomem, nostack),
        );
    }
    frame.set_hypervisor_metadata(value2, instruction);
}

pub(crate) fn restore(trap: Trap<'_>) -> ! {
    if crate::power::is_terminal() {
        crate::power::halt();
    }
    let frame = trap.frame as *mut Frame;
    let stack_top = trap.stack_top;
    if stack::primary_frame(stack_top) != Some(frame as usize) {
        crate::trap::abort();
    }
    // SAFETY: consuming Trap ended the sole Rust frame authority. The exact
    // fixed frame/stack relation was revalidated and restore never returns.
    unsafe { __rustsbi_prototyper_trap_restore(frame, stack_top) }
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
    // SAFETY: closes interrupt entry before abandoning the frame and restores
    // the machine double-trap guard.
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
    fn __rustsbi_prototyper_trap_restore(frame: *mut Frame, stack_top: usize) -> !;
}

core::arch::global_asm!(
    include_str!("entry.S"),
    word_size = const core::mem::size_of::<usize>(),
    stack_size = const stack::STACK_SIZE,
    primary_frame_offset = const stack::PRIMARY_FRAME_OFFSET,
    frame_size = const frame::FRAME_SIZE,
    scratch_sp = const core::mem::size_of::<usize>(),
    scratch_t0 = const core::mem::size_of::<usize>() * 2,
    scratch_t1 = const core::mem::size_of::<usize>() * 3,
    scratch_t2 = const core::mem::size_of::<usize>() * 4,
    scratch_t3 = const core::mem::size_of::<usize>() * 5,
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
