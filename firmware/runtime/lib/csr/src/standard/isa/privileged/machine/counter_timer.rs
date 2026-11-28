//! Programmable machine counter writes and writability probes.

use super::{HpmCounter, Mhpmcounter};
use crate::{AccessError, Write};

/// Replaces the value of the selected programmable counter.
///
/// On RV32, stop accumulation first if the new count must be exact.
/// Readers can observe an intermediate value during the two-word update.
///
/// # Safety
///
/// Execute in M-mode on a hart implementing the counter and serialize its writers.
#[inline]
pub unsafe fn write_mhpmcounter(counter: HpmCounter, value: u64) {
    // SAFETY: The caller owns the selected implemented counter's update.
    match_hpm_counter!(counter, N => unsafe { Mhpmcounter::<N>::write(value) })
}

/// Returns the number of consecutive writable bits starting at bit zero of a stopped counter.
///
/// `N` must be in `3..=31`. On success the counter holds its original value.
/// An illegal instruction during restoration can leave the probed word modified.
///
/// # Safety
///
/// 1. Establish the [machine-trap recovery ABI](crate::recovery).
/// 2. Exclusively own this counter and keep it stopped throughout the probe.
// Keep the 29 probes from accumulating their debug-build locals in one frame.
#[inline(never)]
pub unsafe fn measure_stopped_mhpmcounter<const N: u8>() -> Result<u8, AccessError> {
    macro_rules! exchange {
        ($number:expr, $value:expr) => {{
            let previous: usize;
            // SAFETY: The caller owns the stopped counter and has established recovery.
            unsafe {
                crate::recovery::try_csr_asm!(
                    "csrrw {old}, {csr}, {new}", $number,
                    old = lateout(reg) previous, new = in(reg) $value,
                )
            }?;
            previous
        }};
    }
    let saved = exchange!(Mhpmcounter::<N>::NUMBER, usize::MAX);
    let low = exchange!(Mhpmcounter::<N>::NUMBER, saved);
    #[cfg(target_pointer_width = "32")]
    let bits = {
        // Later RV32 writes need an accessible high word even if it holds no writable bits.
        let saved = exchange!(Mhpmcounter::<N>::HIGH_NUMBER, usize::MAX);
        let high = exchange!(Mhpmcounter::<N>::HIGH_NUMBER, saved);
        ((high as u64) << 32) | low as u64
    };
    #[cfg(target_pointer_width = "64")]
    let bits = low;
    Ok(bits.trailing_ones() as u8)
}
