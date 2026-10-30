//! Machine-level AIA registers.
//!
//! Specification: <https://docs.riscv.org/reference/aia/v1.0/IMSIC.html>,
//! section 2.1.9.
use crate::macros::csr;
use core::num::NonZeroU16;

csr! {
    /// Machine indirect register selector (`miselect`).
    Miselect { 0x350, r,
        /// # Safety
        /// Serialize selection with every indirect-register access on this hart.
        unsafe w
    }

    /// Machine top external interrupt CSR, available when an IMSIC is present.
    /// Ordinary reads observe the top interrupt without acknowledging it.
    /// Use `claim` to atomically read and acknowledge the reported interrupt.
    /// Generic writes and read/modify/write are unavailable.
    Mtopei { 0x35c, r }
}

/// Machine indirect register data CSR identity (`mireg`).
///
/// The value of `miselect` determines which register an access to `mireg`
/// addresses. This type exposes the CSR number only; it does not perform or
/// synchronize indirect accesses.
///
/// See <https://docs.riscv.org/reference/aia/v1.0/IMSIC.html>.
pub enum Mireg {}

impl Mireg {
    /// Architectural CSR number of the machine indirect data register.
    pub const NUMBER: u16 = 0x351;
}

impl Mtopei {
    /// Claims and returns the current hart's top machine external interrupt.
    ///
    /// The returned identity and the acknowledged pending interrupt refer to
    /// the same interrupt. A zero identity means no interrupt was available.
    /// A later arrival can make that interrupt pending again.
    ///
    /// See
    /// <https://docs.riscv.org/reference/aia/v1.0/IMSIC.html#2-1-9-top-external-interrupt-csrs-mtopei-stopei-vstopei>.
    ///
    /// # Safety
    ///
    /// Execute in M-mode on a hart with an initialized machine IMSIC file.
    /// Coordinate claims with interrupt delivery on that hart.
    #[inline]
    pub unsafe fn claim() -> Mtopei {
        #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
        {
            let bits: usize;
            // SAFETY: The caller ensures that the machine interrupt file is
            // initialized and coordinates interrupt claims on this hart.
            unsafe {
                core::arch::asm!(
                    "csrrw {bits}, {csr}, zero",
                    bits = out(reg) bits,
                    csr = const Self::NUMBER,
                    options(nostack),
                );
            }
            Self(bits)
        }
        #[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
        unimplemented!("CSR access requires RISC-V")
    }
}

impl Mtopei {
    /// Returns the reported identity; zero means no interrupt is available.
    #[inline]
    pub const fn iid(self) -> Option<NonZeroU16> {
        NonZeroU16::new(((self.0 >> 16) & 0x7ff) as u16)
    }
}
