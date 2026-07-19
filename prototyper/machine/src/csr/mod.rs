//! Typed machine-CSR probes used during per-hart preparation.

#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
mod riscv;

#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
pub(crate) use riscv::{prepare_counter_access, probe_hypervisor_metadata};

const TIME_COUNTER: usize = 1 << 1;
const MISA_H: usize = 1 << 7;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ProbeError {
    Busy,
    RuntimeUnavailable,
    UnexpectedFault,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PrepareError {
    Counter,
    Readback,
    Unavailable,
    UnexpectedFault,
}

const fn misa_has_hypervisor(misa: usize) -> bool {
    misa & MISA_H != 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::boot::NextMode;

    #[test]
    fn counter_access_masks_are_closed_by_next_mode() {
        let implemented = (1 << 0) | (1 << 2) | (1 << 7);
        assert_eq!(
            supervisor_masks(NextMode::Supervisor, implemented),
            (0x87, 0x2)
        );
        assert_eq!(supervisor_masks(NextMode::User, implemented), (0x2, 0x2));
        assert_eq!(supervisor_masks(NextMode::Machine, implemented), (0, 0));
    }

    #[test]
    fn hypervisor_detection_uses_only_the_standard_misa_bit() {
        assert!(!misa_has_hypervisor(0));
        assert!(misa_has_hypervisor(1 << 7));
        assert!(!misa_has_hypervisor(1 << 6));
    }

    fn supervisor_masks(mode: NextMode, implemented: usize) -> (usize, usize) {
        match mode {
            NextMode::Supervisor => (implemented | TIME_COUNTER, TIME_COUNTER),
            NextMode::User => (TIME_COUNTER, TIME_COUNTER),
            NextMode::Machine => (0, 0),
        }
    }
}
