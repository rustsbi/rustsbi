//! Machine-level top external interrupt register (only with an IMSIC).
//!
//! CSR `mtopei` reports the highest-priority external interrupt that is
//! pending and enabled for machine-level when an IMSIC is present.

use crate::iid::Iid;

riscv::read_only_csr! {
    /// Machine top external interrupt (mtopei).
    Mtopei: 0x35C,
    mask: 0x07FF_07FF,
}

impl Mtopei {
    /// Gets the external interrupt identity of the highest-priority interrupt.
    #[inline]
    pub const fn iid(self) -> Option<Iid> {
        let bits = (self.bits & 0x07FF_0000) >> 16;
        Iid::new(bits as u16)
    }

    /// Gets the 11-bit priority number of the highest-priority external interrupt.
    #[inline]
    pub const fn iprio(self) -> u16 {
        (self.bits & 0x0000_07FF) as u16
    }
}

/// Claims and returns the top machine external interrupt.
///
/// Reading `mtopei` performs the architecturally defined claim operation and
/// clears the returned interrupt from the top-pending state. The complete
/// register value is returned so callers retain both its identity and priority.
#[inline]
pub fn claim() -> Mtopei {
    read()
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
