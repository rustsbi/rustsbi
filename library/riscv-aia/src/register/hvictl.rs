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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hvictl_fields() {
        // Set vti (bit 30), iid=0x123 (bits 27:16), dpr (bit 9), ipriom (bit 8), iprio=0xAB (bits 7:0)
        let bits: usize =
            (1usize << 30) | (0x123usize << 16) | (1usize << 9) | (1usize << 8) | 0xAB;
        let reg = Hvictl::from_bits(bits);
        assert!(reg.vti());
        assert_eq!(reg.iid().map(|i| i.number()), Some(0x123));
        assert!(reg.dpr());
        assert!(reg.ipriom());
        assert_eq!(reg.iprio(), 0xAB);
    }

    #[test]
    fn hvictl_zero_iid() {
        let bits: usize = 0;
        let reg = Hvictl::from_bits(bits);
        assert!(!reg.vti());
        assert!(reg.iid().is_none());
        assert!(!reg.dpr());
        assert!(!reg.ipriom());
        assert_eq!(reg.iprio(), 0);
    }
}
