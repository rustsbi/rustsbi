//! Access to programmable machine event selectors.

use super::{HpmCounter, Mhpmevent};
use crate::{AccessError, Write};
#[cfg(target_pointer_width = "32")]
use {super::Mhpmeventh, crate::Read};

// Each CSR instruction requires a register number known at compile time.
macro_rules! match_hpm_counter {
    ($counter:expr, $index:ident => $access:expr) => {
        match_hpm_counter!($counter, $index => $access;
            3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16,
            17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31)
    };
    ($counter:expr, $index:ident => $access:expr; $($number:literal),+) => {
        match $counter.index() {
            $($number => {
                const $index: u8 = $number;
                $access
            },)+
            _ => unreachable!("HpmCounter guarantees an index in 3..=31"),
        }
    };
}

/// Writes the native word of a programmable machine event selector.
///
/// # Safety
///
/// Establish the [recovery ABI](crate::recovery) and own the selected counter's
/// event configuration on this hart.
#[inline]
pub unsafe fn try_write_mhpmevent(counter: HpmCounter, value: usize) -> Result<(), AccessError> {
    // SAFETY: The caller establishes recovery and owns the selected event configuration.
    match_hpm_counter!(counter, N => unsafe { Mhpmevent::<N>::try_write(value) })
}

/// Reads the optional RV32 high word of a programmable machine event selector.
///
/// # Safety
///
/// Establish the [machine-trap recovery ABI](crate::recovery).
#[cfg(target_pointer_width = "32")]
#[inline]
pub unsafe fn try_read_mhpmeventh(counter: HpmCounter) -> Result<u32, AccessError> {
    // SAFETY: The caller establishes recovery for this side-effect-free standard read.
    match_hpm_counter!(counter, N => unsafe { Mhpmeventh::<N>::try_read() })
}

/// Writes the optional RV32 high word of a programmable machine event selector.
///
/// # Safety
///
/// Establish the [recovery ABI](crate::recovery) and own the selected counter's
/// event configuration on this hart.
#[cfg(target_pointer_width = "32")]
#[inline]
pub unsafe fn try_write_mhpmeventh(counter: HpmCounter, value: u32) -> Result<(), AccessError> {
    // SAFETY: The caller establishes recovery and owns the selected event configuration.
    match_hpm_counter!(counter, N => unsafe { Mhpmeventh::<N>::try_write(value) })
}
