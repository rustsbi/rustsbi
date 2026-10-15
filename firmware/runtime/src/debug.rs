//! Debug-trigger discovery used by the SBI DBTR service.

use crate::csr::instruction;
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
            if instruction!("csrw {csr}, {value}", machine::TriggerSelect::NUMBER, index).is_err() {
                break;
            }
            if instruction!("csrr {value}, {csr}", machine::TriggerSelect::NUMBER, 0) != Ok(index) {
                break;
            }
            let Ok(data) = instruction!("csrr {value}, {csr}", machine::TriggerData1::NUMBER, 0)
            else {
                break;
            };
            if data >> (usize::BITS - 4) & 0xf != 0 {
                count += 1;
            }
        }
        count
    })
}
