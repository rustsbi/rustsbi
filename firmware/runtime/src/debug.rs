//! Debug-trigger discovery used by the SBI DBTR service.

use crate::csr::{csrr, csrw};
use csr::machine;

/// Counts accepted trigger selectors with a nonzero trigger type.
/// Unavailable hardware ends discovery. The walk is bounded to 256 selectors.
pub fn trigger_count() -> usize {
    if !crate::boot::traps_ready() {
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
            if data >> (usize::BITS - 4) & 0xf != 0 {
                count += 1;
            }
        }
        count
    })
}
