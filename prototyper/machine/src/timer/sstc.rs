//! Hart-local Sstc timer mechanism used by AIA installation.

use super::riscv::{prepare_sstc, read_time, write_stimecmp};
use super::{Operations, Timer, TimerError};

static SSTC_TIMER: Operations = Operations {
    prepare_current_hart: prepare_sstc,
    read_time,
    set_deadline: write_stimecmp,
    handle_interrupt: no_machine_interrupt,
};

pub(crate) fn install(harts: &[usize]) -> Result<Timer, TimerError> {
    if harts.is_empty() {
        return Err(TimerError::InvalidHart);
    }
    Ok(Timer::new(&SSTC_TIMER))
}

fn no_machine_interrupt() -> bool {
    false
}
