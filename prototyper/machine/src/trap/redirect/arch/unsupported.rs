//! Fail-stop backend for accidental non-RISC-V trap redirection.

use super::super::super::frame::HypervisorTrap;

pub(in crate::trap) fn read_supervisor_vector() -> Option<usize> {
    panic!("supervisor trap redirection requires a RISC-V target")
}

pub(in crate::trap) fn write_supervisor_trap(
    _: usize,
    _: usize,
    _: usize,
    _: Option<HypervisorTrap>,
) -> bool {
    panic!("supervisor trap redirection requires a RISC-V target")
}
