//! RISC-V supervisor trap CSR commits.

use super::super::super::frame::HypervisorTrap;
use super::super::super::redirect::hypervisor_status;

pub(in crate::trap) fn read_supervisor_vector() -> Option<usize> {
    let vector: usize;
    // SAFETY: `stvec` is a fixed read-only observation here.
    unsafe {
        core::arch::asm!("csrr {vector}, stvec", vector = out(reg) vector, options(nomem, nostack))
    };
    Some(vector)
}

pub(in crate::trap) fn write_supervisor_trap(
    pc: usize,
    cause: usize,
    value: usize,
    hypervisor: Option<HypervisorTrap>,
) -> bool {
    // SAFETY: runtime preparation established supervisor support and the
    // values are immutable copies from the complete machine entry frame.
    unsafe {
        core::arch::asm!(
            "csrw sepc, {pc}",
            "csrw scause, {cause}",
            "csrw stval, {value}",
            pc = in(reg) pc,
            cause = in(reg) cause,
            value = in(reg) value,
            options(nostack),
        )
    };
    hypervisor.is_none_or(write_hypervisor_trap)
}

fn write_hypervisor_trap(metadata: HypervisorTrap) -> bool {
    let original: usize;
    // SAFETY: the per-hart probe proved the fixed H CSRs before publication.
    unsafe {
        core::arch::asm!("csrr {original}, 0x600", original = out(reg) original, options(nomem, nostack))
    };
    let (desired, mask) = hypervisor_status(original, metadata);
    let actual_status: usize;
    let actual_value2: usize;
    let actual_instruction: usize;
    // SAFETY: same probed fixed CSRs; readback verifies WARL effects.
    unsafe {
        core::arch::asm!(
            "csrw 0x600, {status}",
            "csrw 0x643, {value2}",
            "csrw 0x64a, {instruction}",
            "csrr {actual_status}, 0x600",
            "csrr {actual_value2}, 0x643",
            "csrr {actual_instruction}, 0x64a",
            status = in(reg) desired,
            value2 = in(reg) metadata.value2,
            instruction = in(reg) metadata.instruction,
            actual_status = out(reg) actual_status,
            actual_value2 = out(reg) actual_value2,
            actual_instruction = out(reg) actual_instruction,
            options(nomem, nostack),
        )
    };
    actual_status & mask == desired & mask
        && actual_value2 == metadata.value2
        && actual_instruction == metadata.instruction
}
