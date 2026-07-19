//! Hart-local Sstc timer role for the retained AIA configuration.

use alloc::sync::Arc;
use alloc::vec::Vec;

use crate::timer::{Timer, TimerDevice, TimerError};

mod arch;

use arch::{current_hart_id, prepare_current_hart, read_time, write_stimecmp};

pub(super) fn build(harts: Vec<usize>) -> Result<Timer, TimerError> {
    if harts.is_empty() {
        return Err(TimerError::InvalidHart);
    }
    let driver: Arc<dyn TimerDevice> = Arc::new(Sstc { harts });
    Ok(Timer::new(driver))
}

struct Sstc {
    harts: Vec<usize>,
}

impl TimerDevice for Sstc {
    fn prepare_current_hart(&self) -> Result<(), TimerError> {
        let hart_id = current_hart_id();
        if !self.harts.contains(&hart_id) {
            return Err(TimerError::InvalidHart);
        }

        prepare_current_hart()
    }

    fn read_time(&self) -> u64 {
        read_time()
    }

    fn set_compare(&self, hart_id: usize, deadline: u64) {
        if hart_id != current_hart_id() || !self.harts.contains(&hart_id) {
            return;
        }
        write_stimecmp(deadline);
    }

    #[inline(never)]
    fn handle_interrupt(&self) -> bool {
        false
    }
}
