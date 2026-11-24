//! Debug-trigger discovery used by the SBI DBTR service.

use crate::csr::{csrr, csrw};
use csr::machine;

fn is_sbi_usable_trigger(tdata1: usize) -> bool {
    let trigger_type = (tdata1 >> (usize::BITS - 4)) & 0xf;
    let dmode = (tdata1 >> (usize::BITS - 5)) & 1;
    trigger_type != 0 && dmode == 0
}

/// Counts accepted trigger selectors with a nonzero trigger type and `dmode == 0`.
/// Unavailable hardware ends discovery. The walk is bounded to 256 selectors.
pub fn trigger_count() -> usize {
    if !crate::boot::traps_ready() || usize::from(machine::Scratch::read()) != 0 {
        return 0;
    }
    crate::irq::with_local_disabled(|_| {
        let mut count = 0;
        for index in 0..256 {
            if csrw!(machine::TriggerSelect, index).is_err() {
                break;
            }
            if csrr!(machine::TriggerSelect) != Ok(index) {
                break;
            }
            let Ok(data) = csrr!(machine::TriggerData1) else {
                break;
            };
            if is_sbi_usable_trigger(data) {
                count += 1;
            }
        }
        count
    })
}
