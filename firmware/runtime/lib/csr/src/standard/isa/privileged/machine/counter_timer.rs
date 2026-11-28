//! Programmable machine counter writes and writability probes.

#[cfg(target_pointer_width = "32")]
use super::Mhpmeventh;
use super::{HardwareCounter, HpmCounter, Mcountinhibit, Mhpmcounter, Mhpmevent};
use crate::{AccessError, Clear, Read, Set, Write};

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

/// Measures the number of contiguous writable bits starting at bit zero of counter `N`.
///
/// Returns `Ok(0)` if bit zero is unwritable or the hart accepts neither
/// inhibition nor event zero to stop counting.
/// A successful probe restores the counter value and controls.
/// An access error can leave the counter value, inhibit bit, or event selector changed.
/// `N` must be in `3..=31`.
///
/// # Safety
///
/// 1. Establish the [machine-trap recovery ABI](crate::recovery).
/// 2. Exclusively own this counter, its inhibit bit, and its event selector
///    until the probe has restored them or the caller has handled an error.
// Keep the 29 probes from accumulating their debug-build locals in one frame.
#[inline(never)]
pub unsafe fn probe_mhpmcounter<const N: u8>() -> Result<u8, AccessError> {
    let counter =
        const { HardwareCounter::Hpm(HpmCounter::new(N as u16).expect("invalid HPM counter")) };
    // SAFETY: The caller owns the counter under the recovery ABI. Readback of
    // its inhibit bit confirms that counting has stopped before either probe.
    unsafe {
        if let Ok(original) = Mcountinhibit::try_read() {
            if original.contains(counter) {
                return probe_stopped_counter::<N>();
            }
            if Mcountinhibit::try_write(Set(counter)).is_ok() {
                let stopped = Mcountinhibit::try_read().is_ok_and(|bits| bits.contains(counter));
                let width = stopped.then(|| probe_stopped_counter::<N>());
                Mcountinhibit::try_write(Clear(counter))?;
                if let Some(width) = width {
                    return width;
                }
            }
        }
    }
    // SAFETY: The caller owns the selector and counter under the recovery ABI.
    // The probe runs only after event zero reads back from every readable selector word.
    unsafe {
        let event = Mhpmevent::<N>::try_read()?;
        #[cfg(target_pointer_width = "32")]
        let event_high = Mhpmeventh::<N>::try_read().ok();
        Mhpmevent::<N>::try_write(0)?;
        #[cfg(target_pointer_width = "32")]
        if event_high.is_some()
            && let Err(error) = Mhpmeventh::<N>::try_write(0)
        {
            Mhpmevent::<N>::try_write(event)?;
            return Err(error);
        }
        // Restore the selector even if readback or the counter probe traps.
        let result = (|| {
            if Mhpmevent::<N>::try_read()? != 0 {
                return Ok(0);
            }
            #[cfg(target_pointer_width = "32")]
            if event_high.is_some() && Mhpmeventh::<N>::try_read()? != 0 {
                return Ok(0);
            }
            probe_stopped_counter::<N>()
        })();
        #[cfg(target_pointer_width = "32")]
        if let Some(value) = event_high {
            // Restore the high selector while the low word still selects event zero.
            Mhpmeventh::<N>::try_write(value)?;
        }
        Mhpmevent::<N>::try_write(event)?;
        result
    }
}

/// Measures and restores a stopped counter.
///
/// # Safety
///
/// Establish the recovery ABI and keep the owned counter stopped throughout the probe.
// Inlining would duplicate the instruction fixup records at both probe sites.
#[inline(never)]
unsafe fn probe_stopped_counter<const N: u8>() -> Result<u8, AccessError> {
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
        // RV32 counter writes always use both words, even for a narrow counter.
        let saved = exchange!(Mhpmcounter::<N>::HIGH_NUMBER, usize::MAX);
        let high = exchange!(Mhpmcounter::<N>::HIGH_NUMBER, saved);
        ((high as u64) << 32) | low as u64
    };
    #[cfg(target_pointer_width = "64")]
    let bits = low;
    Ok(bits.trailing_ones() as u8)
}
