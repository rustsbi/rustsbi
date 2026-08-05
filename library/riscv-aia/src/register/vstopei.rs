//! Virtual supervisor top external interrupt (only with an IMSIC) (vstopei)

use crate::iid::Iid;

riscv::read_write_csr! {
    /// Virtual supervisor top external interrupt register.
    Vstopei: 0x25C,
    mask: 0x07FF_07FF,
}

impl Vstopei {
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
    fn vstopei_parsing_none() {
        // zero iid should yield None
        let bits: usize = 0; // iid==0, iprio==0
        let reg = Vstopei::from_bits(bits);
        assert_eq!(reg.iprio(), 0);
        assert!(reg.iid().is_none());
    }

    #[test]
    fn vstopei_parsing_high_priority_bits() {
        let iid_num: u16 = 0x523;
        let iprio: u16 = 0x6A5;
        let bits = ((iid_num as usize) << 16) | iprio as usize;
        let reg = Vstopei::from_bits(bits);
        assert_eq!(reg.iid().map(|iid| iid.number()), Some(iid_num));
        assert_eq!(reg.iprio(), iprio);
    }
}
