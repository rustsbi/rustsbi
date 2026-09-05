//! Machine-level top external interrupt register (only with an IMSIC).
//!
//! CSR `mtopei` reports the highest-priority external interrupt that is
//! pending and enabled for machine-level when an IMSIC is present.

use crate::iid::Iid;

#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
const CSR_MTOPEI: u16 = 0x35c;

const FIELD_MASK: usize = 0x07ff;
const IDENTITY_SHIFT: u32 = 16;
const REGISTER_MASK: usize = (FIELD_MASK << IDENTITY_SHIFT) | FIELD_MASK;

riscv::csr! {
    /// Machine top external interrupt (mtopei).
    Mtopei,
    REGISTER_MASK
}

impl Mtopei {
    /// Gets the external interrupt identity of the highest-priority interrupt.
    #[inline]
    pub const fn iid(self) -> Option<Iid> {
        let bits = (self.bits & (FIELD_MASK << IDENTITY_SHIFT)) >> IDENTITY_SHIFT;
        Iid::new(bits as u16)
    }

    /// Gets the 11-bit priority number of the highest-priority external interrupt.
    #[inline]
    pub const fn iprio(self) -> u16 {
        (self.bits & FIELD_MASK) as u16
    }
}

/// Reads `mtopei` without claiming the reported interrupt.
#[inline]
pub fn read() -> Mtopei {
    try_read().expect("mtopei is unavailable on this target")
}

/// Attempts to read `mtopei` without claiming the reported interrupt.
#[inline]
pub fn try_read() -> riscv::result::Result<Mtopei> {
    #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
    {
        let bits: usize;
        // SAFETY: reading this CSR has no memory-safety preconditions. A caller
        // must still ensure that the current hart implements an IMSIC.
        unsafe {
            core::arch::asm!(
                "csrrs {bits}, {csr}, zero",
                bits = out(reg) bits,
                csr = const CSR_MTOPEI,
                options(nomem, nostack),
            );
        }
        Ok(Mtopei::from_bits(bits))
    }

    #[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
    {
        Err(riscv::result::Error::Unimplemented)
    }
}

/// Claims and returns the top machine external interrupt.
///
/// The read and write are performed atomically, as required by RISC-V AIA 1.0
/// §2.1.9, so the interrupt reported by the read is the one whose pending bit
/// is cleared.
#[inline]
pub fn claim() -> Mtopei {
    #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
    {
        let bits: usize;
        // SAFETY: `mtopei` is accessed with the architecturally prescribed
        // atomic claim sequence. Callers use this only when an IMSIC is
        // present for the current hart.
        unsafe {
            core::arch::asm!(
                "csrrw {bits}, {csr}, zero",
                bits = out(reg) bits,
                csr = const CSR_MTOPEI,
                options(nomem, nostack),
            );
        }
        Mtopei::from_bits(bits)
    }

    #[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
    {
        panic!("mtopei is unavailable on this target")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mtopei_parse() {
        let iid_num: u16 = 0x5a3;
        let iprio: u16 = 0x6d2;
        let bits: usize = ((iid_num as usize) << 16) | iprio as usize;
        let reg = Mtopei::from_bits(bits);
        assert_eq!(reg.iprio(), iprio);
        assert_eq!(reg.iid().map(|iid| iid.number()), Some(iid_num));
    }

    #[test]
    fn zero_identity_reports_no_interrupt() {
        let reg = Mtopei::from_bits(0x7ff);
        assert_eq!(reg.iid(), None);
        assert_eq!(reg.iprio(), 0x7ff);
    }

    #[test]
    fn reserved_bits_are_masked() {
        let reg = Mtopei::from_bits(usize::MAX);
        assert_eq!(reg.bits(), 0x07ff_07ff);
        assert_eq!(reg.iid().map(|iid| iid.number()), Some(0x7ff));
        assert_eq!(reg.iprio(), 0x7ff);
    }
}
