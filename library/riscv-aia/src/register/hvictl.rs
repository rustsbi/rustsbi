//! Hypervisor virtual interrupt control (hvictl)

use crate::iid::MajorIid;

riscv::read_write_csr! {
    /// Hypervisor virtual interrupt control.
    Hvictl: 0x609,
    mask: 0x4FFF_03FF,
}

impl Hvictl {
    /// IID field (bits 27:16) — interrupt identity for a virtual interrupt.
    #[inline]
    pub const fn iid(self) -> MajorIid {
        let bits = ((self.bits >> 16) & 0x0FFF) as u16;
        MajorIid::new(bits)
    }

    /// Set IID field (bits 27:16).
    #[inline]
    pub const fn set_iid(&mut self, value: MajorIid) {
        self.bits = (self.bits & !(0x0FFFusize << 16)) | ((value.number() as usize) << 16);
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
    fn hvictl_boolean_fields_are_one_hot() {
        let vti = Hvictl::from_bits(1 << 30);
        assert!(vti.vti());
        assert!(!vti.dpr());
        assert!(!vti.ipriom());

        let dpr = Hvictl::from_bits(1 << 9);
        assert!(!dpr.vti());
        assert!(dpr.dpr());
        assert!(!dpr.ipriom());

        let ipriom = Hvictl::from_bits(1 << 8);
        assert!(!ipriom.vti());
        assert!(!ipriom.dpr());
        assert!(ipriom.ipriom());
    }

    #[test]
    fn hvictl_value_fields() {
        let mut reg = Hvictl::from_bits(0);
        reg.set_iid(MajorIid::new(0x123));
        reg.set_iprio(0xAB);
        assert_eq!(reg.iid().number(), 0x123);
        assert_eq!(reg.iprio(), 0xAB);
        assert_eq!(reg.bits(), (0x123usize << 16) | 0xAB);
    }

    #[test]
    fn hvictl_zero_iid() {
        let bits: usize = 0;
        let reg = Hvictl::from_bits(bits);
        assert!(!reg.vti());
        assert_eq!(reg.iid().number(), 0);
        assert!(!reg.dpr());
        assert!(!reg.ipriom());
        assert_eq!(reg.iprio(), 0);
    }

    #[test]
    fn hvictl_mask() {
        assert_eq!(Hvictl::BITMASK, 0x4FFF_03FF);
        assert_eq!(Hvictl::from_bits(usize::MAX).bits(), 0x4FFF_03FF);
    }
}
