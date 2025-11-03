//! Supervisor top interrupt (stopi)

riscv::read_only_csr! {
    /// Supervisor top interrupt register.
    Stopi: 0xDB0,
    mask: 0x0FFF_00FF,
}

impl Stopi {
    /// Get the major identity number of the highest-priority interrupt.
    #[inline]
    pub const fn iid(self) -> Option<crate::Iid> {
        let bits = (self.bits & 0x0FFF_0000) >> 16;
        crate::Iid::new(bits as u16)
    }

    /// Indicates the priority number of the highest-priority interrupt.
    #[inline]
    pub const fn iprio(self) -> u8 {
        (self.bits & 0x0000_00FF) as u8
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stopi_parsing() {
        let iid_num: u16 = 0x123;
        let iprio: u8 = 0x7;
        let bits: usize = ((iid_num as usize) << 16) | (iprio as usize);
        let reg = Stopi::from_bits(bits);
        assert_eq!(reg.iprio(), iprio);
        assert_eq!(reg.iid().map(|i| i.number()), Some(iid_num));
    }

    #[test]
    fn stopi_zero_iid() {
        let reg = Stopi::from_bits(0);
        assert!(reg.iid().is_none());
        assert_eq!(reg.iprio(), 0);
    }
}
