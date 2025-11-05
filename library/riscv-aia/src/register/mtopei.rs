//! Machine-level top external interrupt register (only with an IMSIC).
//! Machine-level top external interrupt register (only with an IMSIC).
//!
//! CSR `mtopei` reports the highest-priority external interrupt that is
//! pending and enabled for machine-level when an IMSIC is present. Provide a
//! small typed wrapper similar to `Mtopi` for convenient field extraction.

use crate::Iid;

riscv::read_only_csr! {
    /// Machine top external interrupt (mtopei).
    Mtopei: 0x35C,
    mask: 0x0FFF_00FF,
}

impl Mtopei {
    /// Get the major identity number of the highest-priority external interrupt.
    #[inline]
    pub const fn iid(self) -> Option<Iid> {
        let bits = (self.bits & 0x0FFF_0000) >> 16;
        Iid::new(bits as u16)
    }

    /// Indicates the priority number of the highest-priority external interrupt.
    #[inline]
    pub const fn iprio(self) -> u8 {
        (self.bits & 0x0000_00FF) as u8
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mtopei_parsing() {
        let iid_num: u16 = 1;
        let iprio: u8 = 0xAA;
        let bits: usize = ((iid_num as usize) << 16) | (iprio as usize);
        let reg = Mtopei::from_bits(bits);
        assert_eq!(reg.iprio(), iprio);
        assert_eq!(reg.iid().map(|i| i.number()), Some(iid_num));
    }
}
