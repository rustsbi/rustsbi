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
