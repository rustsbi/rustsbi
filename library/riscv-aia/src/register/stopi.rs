//! Supervisor top interrupt (stopi)

use crate::iid::MajorIid;

riscv::read_only_csr! {
    /// Supervisor top interrupt register.
    Stopi: 0xDB0,
    mask: 0x0FFF_00FF,
}

impl Stopi {
    /// Get the major identity number of the highest-priority interrupt.
    #[inline]
    pub const fn iid(self) -> Option<MajorIid> {
        match self.bits {
            0 => None,
            _ => {
                let major_iid = ((self.bits & 0x0FFF_0000) >> 16) as u16;
                Some(MajorIid::new(major_iid))
            }
        }
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
        let iid_num: u16 = 0xABC;
        let iprio: u8 = 0x7;
        let bits: usize = ((iid_num as usize) << 16) | (iprio as usize);
        let reg = Stopi::from_bits(bits);
        assert_eq!(reg.iprio(), iprio);
        assert_eq!(reg.iid().map(|i| i.number()), Some(iid_num));
    }

    #[test]
    fn stopi_zero_csr() {
        let reg = Stopi::from_bits(0);
        assert!(reg.iid().is_none());
        assert_eq!(reg.iprio(), 0);
    }

    #[test]
    fn stopi_zero_iid_with_nonzero_priority() {
        let reg = Stopi::from_bits(0x01);
        assert_eq!(reg.iid().map(|iid| iid.number()), Some(0));
        assert_eq!(reg.iprio(), 1);
    }
}
