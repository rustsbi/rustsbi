//! Machine-level AIA registers.
//!
//! Specification: <https://docs.riscv.org/reference/aia/v1.0/IMSIC.html>,
//! section 2.1.9.
use crate::macros::csr;
#[cfg(any(test, target_arch = "riscv32", target_arch = "riscv64"))]
use crate::value::Value;
use core::num::NonZeroU16;

csr! {
    /// The value observed or claimed from the machine interrupt file.
    pub struct MtopeiValue(usize);

    /// Machine top external interrupt CSR, available when an IMSIC is present.
    /// Ordinary reads observe the top interrupt without acknowledging it.
    /// Use `claim` to atomically read and acknowledge the reported interrupt.
    /// Generic writes and read/modify/write are unavailable.
    Mtopei { 0x35c, r: MtopeiValue }
}

impl Mtopei {
    /// Claims and returns the current hart's top machine external interrupt.
    ///
    /// Executes one `csrrw rd, mtopei, x0`.
    /// It clears the pending bit for the returned identity, even if another
    /// interrupt arrives during the operation.
    /// A zero identity means no interrupt was available.
    /// An ordinary read followed by a separate write cannot provide this
    /// guarantee.
    ///
    /// Use after discovering and initializing the current hart's machine IMSIC
    /// file.
    /// Inaccessible hardware traps.
    /// Concurrent claim paths can consume each other's interrupts; the device's
    /// delivery policy must coordinate those paths.
    ///
    /// See
    /// <https://docs.riscv.org/reference/aia/v1.0/IMSIC.html#2-1-9-top-external-interrupt-csrs-mtopei-stopei-vstopei>.
    #[inline]
    pub fn claim() -> MtopeiValue {
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
            MtopeiValue::from_bits(bits)
        }
        #[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
        panic!("CSR access requires RISC-V")
    }
}

impl MtopeiValue {
    /// Returns the reported identity; zero means no interrupt is available.
    #[inline]
    pub const fn iid(self) -> Option<NonZeroU16> {
        NonZeroU16::new(((self.0 >> 16) & 0x7ff) as u16)
    }

    /// Returns the reported 11-bit interrupt priority.
    #[inline]
    pub const fn iprio(self) -> u16 {
        (self.0 & 0x7ff) as u16
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_and_priority_ignore_reserved_bits() {
        let bits = (0x5a3 << 16) | 0x6d2 | (usize::MAX & !0x07ff_07ff);
        let value = MtopeiValue::from_bits(bits);
        assert_eq!(value.iid().map(NonZeroU16::get), Some(0x5a3));
        assert_eq!(value.iprio(), 0x6d2);
        assert_eq!(usize::from(value), bits);
    }

    #[test]
    fn zero_identity_means_no_interrupt_even_with_nonzero_priority() {
        let value = MtopeiValue::from_bits(0x7ff);
        assert_eq!(value.iid(), None);
        assert_eq!(value.iprio(), 0x7ff);
    }
}
