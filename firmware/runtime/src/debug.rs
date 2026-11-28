//! Debug-trigger discovery used by the SBI DBTR service.

use csr::{
    Read, Write,
    standard::isa::privileged::debug::{Tdata1, Tselect},
};

/// Counts triggers with `dmode == 0`, starting at selector zero and probing at most 256.
///
/// Returns zero if recovery is unavailable.
/// Enumeration ends at the first rejected selector, zero trigger type or failed CSR access.
/// If restoring `tselect` fails, the hart can retain the last selector probed.
pub fn trigger_count() -> usize {
    if !crate::recovery::is_ready() {
        return 0;
    }
    // SAFETY: Runtime trap entry is ready with mscratch and MPRV clear.
    // Local interrupt exclusion serializes the selector and data accesses.
    crate::irq::with_local_disabled(|_| unsafe {
        let Ok(saved_tselect) = Tselect::try_read() else {
            return 0;
        };
        let mut count = 0;
        for index in 0..256 {
            if Tselect::try_write(index).is_err() {
                break;
            }
            if Tselect::try_read() != Ok(index) {
                break;
            }
            let Ok(data) = Tdata1::try_read() else {
                break;
            };
            if data.trigger_type() == 0 {
                break;
            }
            if !data.dmode() {
                count += 1;
            }
        }
        let _ = Tselect::try_write(saved_tselect);
        count
    })
}
