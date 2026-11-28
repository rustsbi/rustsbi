//! Indexed access to machine physical memory protection words.

use super::{Pmpaddr, Pmpcfg};
use crate::{Read, Write};

macro_rules! pmp_word_access {
    ($read:ident, $write:ident, $register:ident; $($index:literal),+ $(,)?) => {
        #[doc = concat!("Reads [`", stringify!($register), "`] at the given CSR index.")]
        ///
        /// Returns `None` for an index unavailable at this XLEN.
        /// An unimplemented register at a valid index traps normally.
        #[inline]
        pub fn $read(index: u8) -> Option<usize> {
            match index {
                $($index => Some($register::<$index>::read()),)+
                _ => None,
            }
        }

        #[doc = concat!("Writes [`", stringify!($register), "`] at the given CSR index.")]
        ///
        /// Returns `false` for an index unavailable at this XLEN.
        /// A `true` result means the instruction completed.
        /// Locked entries can ignore the write.
        ///
        /// # Safety
        ///
        #[doc = concat!("Execute in M-mode on a hart implementing the selected [`",
            stringify!($register), "`].")]
        /// Preserve the permissions required by live memory accesses throughout the
        /// update and synchronize address translation before relying on the new protection.
        #[inline]
        pub unsafe fn $write(index: u8, value: usize) -> bool {
            match index {
                $($index => {
                    // SAFETY: The caller establishes the selected PMP word's write invariants.
                    unsafe { $register::<$index>::write(value) };
                    true
                },)+
                _ => false,
            }
        }
    };
}

#[cfg(target_pointer_width = "32")]
pmp_word_access!(read_pmpcfg, write_pmpcfg, Pmpcfg; 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15);
#[cfg(target_pointer_width = "64")]
pmp_word_access!(read_pmpcfg, write_pmpcfg, Pmpcfg; 0, 2, 4, 6, 8, 10, 12, 14);

pmp_word_access!(read_pmpaddr, write_pmpaddr, Pmpaddr;
    0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15,
    16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31,
    32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47,
    48, 49, 50, 51, 52, 53, 54, 55, 56, 57, 58, 59, 60, 61, 62, 63,
);
