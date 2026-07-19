//! Fail-stop backend for accidental non-RISC-V Sstc use.

use crate::timer::TimerError;

fn unavailable() -> ! {
    panic!("Sstc operations require a RISC-V target")
}

pub(in crate::drivers::sstc) fn prepare_current_hart() -> Result<(), TimerError> {
    unavailable()
}

pub(in crate::drivers::sstc) fn current_hart_id() -> usize {
    unavailable()
}

pub(in crate::drivers::sstc) fn read_time() -> u64 {
    unavailable()
}

pub(in crate::drivers::sstc) fn write_stimecmp(_: u64) {
    unavailable()
}
