//! Supervisor top external interrupt (only with an IMSIC) (stopei)

use crate::iid::Iid;

riscv::read_only_csr! {
    /// Supervisor top external interrupt register.
    Stopei: 0x15C,
    mask: 0x07FF_07FF,
}

impl Stopei {
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
    fn stopei_parsing() {
        let iid_num: u16 = 2047; // max allowed
        let iprio: u16 = 0x7FF;
        let bits: usize = ((iid_num as usize) << 16) | (iprio as usize);
        let reg = Stopei::from_bits(bits);
        assert_eq!(reg.iprio(), iprio);
        assert_eq!(reg.iid().map(|i| i.number()), Some(iid_num));
    }
}
