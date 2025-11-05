//! Virtual supervisor top interrupt (vstopi)

riscv::read_only_csr! {
    /// Virtual supervisor top interrupt register.
    Vstopi: 0xEB0,
    mask: 0x0FFF_00FF,
}

impl Vstopi {
    #[inline]
    pub const fn iid(self) -> Option<crate::Iid> {
        let bits = (self.bits & 0x0FFF_0000) >> 16;
        crate::Iid::new(bits as u16)
    }

    #[inline]
    pub const fn iprio(self) -> u8 {
        (self.bits & 0x0000_00FF) as u8
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vstopi_parsing() {
        let iid_num: u16 = 0x456;
        let iprio: u8 = 0xFF;
        let bits: usize = ((iid_num as usize) << 16) | (iprio as usize);
        let reg = Vstopi::from_bits(bits);
        assert_eq!(reg.iprio(), iprio);
        assert_eq!(reg.iid().map(|i| i.number()), Some(iid_num));
    }

    #[test]
    fn vstopi_zero_iid() {
        let reg = Vstopi::from_bits(0);
        assert!(reg.iid().is_none());
        assert_eq!(reg.iprio(), 0);
    }
}
