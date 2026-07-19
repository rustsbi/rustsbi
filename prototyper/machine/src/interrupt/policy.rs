//! Typed interrupt and exception delegation policy.

use bitflags::bitflags;

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
pub(super) struct Delegation {
    interrupts: Interrupts,
    exceptions: Exceptions,
}

impl Delegation {
    pub(super) const NONE: Self = Self {
        interrupts: Interrupts::empty(),
        exceptions: Exceptions::empty(),
    };

    pub(super) const SUPERVISOR: Self = Self {
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

    pub(super) const fn from_raw(interrupts: usize, exceptions: usize) -> Self {
        Self {
            interrupts: Interrupts::from_bits_retain(interrupts),
            exceptions: Exceptions::from_bits_retain(exceptions),
        }
    }

    pub(super) const fn interrupt_bits(self) -> usize {
        self.interrupts.bits()
    }

    pub(super) const fn exception_bits(self) -> usize {
        self.exceptions.bits()
    }

    #[cfg(feature = "mtest")]
    pub(super) fn contains(self, other: Self) -> bool {
        self.interrupts.contains(other.interrupts) && self.exceptions.contains(other.exceptions)
    }

    pub(super) fn excludes_machine_local(self) -> bool {
        self.interrupts.bits() & Self::MACHINE_LOCAL.interrupts.bits() == 0
            && self.exceptions.bits() & Self::MACHINE_LOCAL.exceptions.bits() == 0
    }

    pub(super) fn readback_valid(fixed: Self, actual: Self, requested: Self) -> bool {
        fixed.excludes_machine_local()
            && actual.interrupts == fixed.interrupts.union(requested.interrupts)
            && actual.exceptions == fixed.exceptions.union(requested.exceptions)
    }
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

        let fixed_machine_timer = Delegation::from_raw(1 << 7, 0);
        assert!(!Delegation::readback_valid(
            fixed_machine_timer,
            Delegation::from_raw(
                fixed_machine_timer.interrupt_bits() | Delegation::SUPERVISOR.interrupt_bits(),
                Delegation::SUPERVISOR.exception_bits(),
            ),
            Delegation::SUPERVISOR,
        ));
    }
}
