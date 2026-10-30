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
/// Runtime groups selector/data operations under local IRQ exclusion.
/// The selected register determines the access effects and ownership contract.
pub enum Mireg {}

impl Mireg {
    /// Architectural CSR number of the machine indirect data register.
    pub const NUMBER: u16 = 0x351;
}

impl Mtopei {
    /// Claims and returns the current hart's top machine external interrupt.
    ///
    /// Executes one `csrrw rd, mtopei, x0`.
    /// It reads the top identity and clears that identity's pending bit atomically.
    /// A later arrival can make that interrupt pending again.
    /// A zero identity means no interrupt was available.
    /// An ordinary read followed by a separate write cannot provide this
    /// guarantee.
    ///
    /// Use only on a hart with a machine IMSIC file. Coordinate claims with
    /// interrupt-file initialization and delivery.
    /// Inaccessible hardware traps.
    /// Concurrent claim paths can consume each other's interrupts; the device's
    /// delivery policy must coordinate those paths.
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
            // SAFETY: the fixed atomic claim only acknowledges the interrupt
            // it reports; it changes no execution-environment configuration.
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
