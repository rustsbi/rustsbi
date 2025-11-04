//! Hypervisor virtual interrupt control (hvictl)

use crate::Iid;

riscv::read_write_csr! {
    /// Hypervisor virtual interrupt control.
    Hvictl: 0x609,
    mask: 0xFFFF_FFFF,
}

impl Hvictl {
    /// IID field (bits 27:16) — interrupt identity for a virtual interrupt.
    #[inline]
    pub const fn iid(self) -> Option<Iid> {
        let bits = ((self.bits >> 16) & 0x0FFF) as u16;
        Iid::new(bits)
    }

    /// IPRIO field (bits 7:0).
    #[inline]
    pub const fn iprio(&self) -> u8 {
        (self.bits & 0xFF) as u8
    }

    /// Set IPRIO field (bits 7:0).
    #[inline]
    pub const fn set_iprio(&mut self, value: u8) {
        self.bits = (self.bits & !0xFF) | (value as usize)
    }
}

riscv::read_write_csr_field! {
    Hvictl,
    /// Virtual Trap Interrupt (VTI) control.
    vti: 30,
}

riscv::read_write_csr_field! {
    Hvictl,
    /// Default Priority Rank (DPR) bit.
    dpr: 9,
}

riscv::read_write_csr_field! {
    Hvictl,
    /// IPRIO mode bit.
    ipriom: 8,
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
