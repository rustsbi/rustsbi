//! RISC-V delegation CSR installation and readback.

use crate::boot::NextMode;

use super::InterruptError;
use super::policy::Delegation;

pub(crate) fn prepare(mode: NextMode) -> Result<(), InterruptError> {
    let requested = match mode {
        NextMode::Supervisor => Delegation::SUPERVISOR,
        NextMode::User | NextMode::Machine => Delegation::NONE,
    };
    let fixed_interrupts: usize;
    let fixed_exceptions: usize;
    let actual_interrupts: usize;
    let actual_exceptions: usize;
    // SAFETY: both CSR choices and all requested causes come from the closed
    // typed policy. A zero write first discovers only WARL bits that hardware
    // forces to one; the second readback must equal that baseline plus policy.
    unsafe {
        core::arch::asm!(
            "csrw mideleg, zero",
            "csrr {fixed_interrupts}, mideleg",
            "csrw medeleg, zero",
            "csrr {fixed_exceptions}, medeleg",
            "csrw mideleg, {interrupts}",
            "csrr {actual_interrupts}, mideleg",
            "csrw medeleg, {exceptions}",
            "csrr {actual_exceptions}, medeleg",
            interrupts = in(reg) requested.interrupt_bits(),
            exceptions = in(reg) requested.exception_bits(),
            fixed_interrupts = out(reg) fixed_interrupts,
            fixed_exceptions = out(reg) fixed_exceptions,
            actual_interrupts = out(reg) actual_interrupts,
            actual_exceptions = out(reg) actual_exceptions,
            options(nostack),
        )
    };
    let fixed = Delegation::from_raw(fixed_interrupts, fixed_exceptions);
    let actual = Delegation::from_raw(actual_interrupts, actual_exceptions);
    if !Delegation::readback_valid(fixed, actual, requested) {
        return Err(InterruptError::Readback);
    }
    Ok(())
}

#[crate::mtest]
fn machine_owned_causes_remain_local() {
    let interrupts: usize;
    let exceptions: usize;
    // SAFETY: these fixed read-only observations change no architectural
    // state and run after the production delegation policy was verified.
    unsafe {
        core::arch::asm!(
            "csrr {interrupts}, mideleg",
            "csrr {exceptions}, medeleg",
            interrupts = out(reg) interrupts,
            exceptions = out(reg) exceptions,
            options(nomem, nostack),
        )
    };
    let actual = Delegation::from_raw(interrupts, exceptions);
    assert!(actual.excludes_machine_local());
    assert!(actual.contains(Delegation::SUPERVISOR));
}
