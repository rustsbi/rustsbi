//! Machine-level top external interrupt register (only with an IMSIC).
//!
//! CSR `mtopei` reports the highest-priority external interrupt that is
//! pending and enabled for machine-level when an IMSIC is present. Provide a
//! small typed wrapper similar to `Mtopi` for convenient field extraction.

use crate::iid::Iid;

riscv::read_write_csr! {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mtopei_parsing() {
        let iid_num: u16 = 0x5A3;
        let iprio: u16 = 0x6D2;
        let bits: usize = ((iid_num as usize) << 16) | (iprio as usize);
        let reg = Mtopei::from_bits(bits);
        assert_eq!(reg.iprio(), iprio);
        assert_eq!(reg.iid().map(|i| i.number()), Some(iid_num));
    }
}
