//! Hypervisor virtual interrupt control (hvictl)

riscv::read_write_csr! {
    /// Hypervisor virtual interrupt control.
    Hvictl: 0x609,
    mask: 0xFFFF_FFFF_FFFF_FFFF,
}

impl Hvictl {
    /// Virtual Trap Interrupt (VTI) bit (bit 30).
    #[inline]
    pub const fn vti(self) -> bool {
        ((self.bits >> 30) & 1) != 0
    }

    /// IID field (bits 27:16) — interrupt identity for a virtual interrupt.
    #[inline]
    pub const fn iid(self) -> Option<crate::Iid> {
        let bits = ((self.bits >> 16) & 0x0FFF) as u16;
        crate::Iid::new(bits)
    }

    /// Default Priority Rank (DPR) bit (bit 9).
    #[inline]
    pub const fn dpr(self) -> bool {
        ((self.bits >> 9) & 1) != 0
    }

    /// IPRIO mode bit (bit 8).
    #[inline]
    pub const fn ipriom(self) -> bool {
        ((self.bits >> 8) & 1) != 0
    }

    /// IPRIO field (bits 7:0).
    #[inline]
    pub const fn iprio(self) -> u8 {
        (self.bits & 0xFF) as u8
    }
}
