//! Fail-stop backend for accidental non-RISC-V CLINT use.

fn unavailable() -> ! {
    panic!("CLINT operations require a RISC-V target")
}

pub(in crate::drivers::clint) fn device_fence() {
    unavailable()
}

pub(in crate::drivers::clint) fn enable_machine_timer() {
    unavailable()
}

pub(in crate::drivers::clint) fn current_hart_id() -> usize {
    unavailable()
}

pub(in crate::drivers::clint) fn enable_machine_software_interrupt() {
    unavailable()
}

pub(in crate::drivers::clint) fn manifest_supervisor_timer() {
    unavailable()
}

pub(in crate::drivers::clint) fn read_time_csr() -> u64 {
    unavailable()
}
