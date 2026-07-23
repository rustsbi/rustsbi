//! Typed interrupt and exception delegation owned by trap routing.

use bitflags::bitflags;

use crate::boot::NextMode;

bitflags! {
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    struct Interrupts: usize {
        const SUPERVISOR_SOFTWARE = 1 << 1;
        const MACHINE_SOFTWARE = 1 << 3;
        const SUPERVISOR_TIMER = 1 << 5;
        const MACHINE_TIMER = 1 << 7;
        const SUPERVISOR_EXTERNAL = 1 << 9;
        const MACHINE_EXTERNAL = 1 << 11;
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    struct Exceptions: usize {
        const INSTRUCTION_MISALIGNED = 1 << 0;
        const INSTRUCTION_ACCESS = 1 << 1;
        const ILLEGAL_INSTRUCTION = 1 << 2;
        const BREAKPOINT = 1 << 3;
        const LOAD_MISALIGNED = 1 << 4;
        const LOAD_ACCESS = 1 << 5;
        const STORE_MISALIGNED = 1 << 6;
        const STORE_ACCESS = 1 << 7;
        const USER_ECALL = 1 << 8;
        const SUPERVISOR_ECALL = 1 << 9;
        const INSTRUCTION_PAGE = 1 << 12;
        const LOAD_PAGE = 1 << 13;
        const STORE_PAGE = 1 << 15;
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Delegation {
    interrupts: Interrupts,
    exceptions: Exceptions,
}

impl Delegation {
    const NONE: Self = Self {
        interrupts: Interrupts::empty(),
        exceptions: Exceptions::empty(),
    };

    const SUPERVISOR: Self = Self {
        interrupts: Interrupts::SUPERVISOR_SOFTWARE
            .union(Interrupts::SUPERVISOR_TIMER)
            .union(Interrupts::SUPERVISOR_EXTERNAL),
        exceptions: Exceptions::INSTRUCTION_MISALIGNED
            .union(Exceptions::INSTRUCTION_ACCESS)
            .union(Exceptions::BREAKPOINT)
            .union(Exceptions::LOAD_ACCESS)
            .union(Exceptions::STORE_ACCESS)
            .union(Exceptions::USER_ECALL)
            .union(Exceptions::INSTRUCTION_PAGE)
            .union(Exceptions::LOAD_PAGE)
            .union(Exceptions::STORE_PAGE),
    };

    const MACHINE_LOCAL: Self = Self {
        interrupts: Interrupts::MACHINE_SOFTWARE
            .union(Interrupts::MACHINE_TIMER)
            .union(Interrupts::MACHINE_EXTERNAL),
        exceptions: Exceptions::ILLEGAL_INSTRUCTION
            .union(Exceptions::LOAD_MISALIGNED)
            .union(Exceptions::STORE_MISALIGNED)
            .union(Exceptions::SUPERVISOR_ECALL),
    };

    const fn from_raw(interrupts: usize, exceptions: usize) -> Self {
        Self {
            interrupts: Interrupts::from_bits_retain(interrupts),
            exceptions: Exceptions::from_bits_retain(exceptions),
        }
    }

    const fn interrupt_bits(self) -> usize {
        self.interrupts.bits()
    }

    const fn exception_bits(self) -> usize {
        self.exceptions.bits()
    }

    #[cfg(feature = "mtest")]
    fn contains(self, other: Self) -> bool {
        self.interrupts.contains(other.interrupts) && self.exceptions.contains(other.exceptions)
    }

    fn excludes_machine_local(self) -> bool {
        self.interrupts.bits() & Self::MACHINE_LOCAL.interrupts.bits() == 0
            && self.exceptions.bits() & Self::MACHINE_LOCAL.exceptions.bits() == 0
    }

    fn readback_valid(fixed: Self, actual: Self, requested: Self) -> bool {
        fixed.excludes_machine_local()
            && actual.interrupts == fixed.interrupts.union(requested.interrupts)
            && actual.exceptions == fixed.exceptions.union(requested.exceptions)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DelegationError {
    Readback,
}

pub(crate) fn prepare(mode: NextMode) -> Result<(), DelegationError> {
    let requested = match mode {
        NextMode::Supervisor => Delegation::SUPERVISOR,
        NextMode::User | NextMode::Machine => Delegation::NONE,
    };
    let fixed_interrupts: usize;
    let fixed_exceptions: usize;
    let actual_interrupts: usize;
    let actual_exceptions: usize;
    // SAFETY: both CSR choices and every requested cause come from the closed
    // policy. Readback proves the installed value before handoff.
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
    Delegation::readback_valid(fixed, actual, requested)
        .then_some(())
        .ok_or(DelegationError::Readback)
}

#[cfg(feature = "mtest")]
#[crate::mtest]
fn machine_owned_causes_remain_local() {
    let interrupts: usize;
    let exceptions: usize;
    // SAFETY: fixed read-only observations after delegation was installed.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supervisor_and_machine_owned_causes_are_disjoint() {
        assert!(Delegation::SUPERVISOR.excludes_machine_local());
    }

    #[test]
    fn supervisor_interrupts_have_the_standard_cause_numbers() {
        assert_eq!(
            Delegation::SUPERVISOR.interrupt_bits(),
            (1 << 1) | (1 << 5) | (1 << 9)
        );
    }

    #[test]
    fn readback_accepts_only_fixed_warl_bits_plus_closed_policy() {
        let fixed = Delegation::from_raw((1 << 2) | (1 << 6) | (1 << 10), 1 << 12);
        let expected = Delegation::from_raw(
            fixed.interrupt_bits() | Delegation::SUPERVISOR.interrupt_bits(),
            fixed.exception_bits() | Delegation::SUPERVISOR.exception_bits(),
        );
        assert!(Delegation::readback_valid(
            fixed,
            expected,
            Delegation::SUPERVISOR
        ));

        let unexpected = Delegation::from_raw(
            expected.interrupt_bits() | (1 << 15),
            expected.exception_bits(),
        );
        assert!(!Delegation::readback_valid(
            fixed,
            unexpected,
            Delegation::SUPERVISOR,
        ));
    }
}
