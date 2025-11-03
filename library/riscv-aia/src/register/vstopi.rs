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
