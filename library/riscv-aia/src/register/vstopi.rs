//! Virtual supervisor top interrupt (vstopi)

use crate::iid::MajorIid;

riscv::read_only_csr! {
    /// Virtual supervisor top interrupt register.
    Vstopi: 0xEB0,
    mask: 0x0FFF_00FF,
}

impl Vstopi {
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

    #[inline]
    pub const fn iprio(self) -> u8 {
        (self.bits & 0x0000_00FF) as u8
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vstopi_parse() {
        let iid_num: u16 = 0xFFF;
        let iprio: u8 = 0xFF;
        let bits: usize = ((iid_num as usize) << 16) | (iprio as usize);
        let reg = Vstopi::from_bits(bits);
        assert_eq!(reg.iprio(), iprio);
        assert_eq!(reg.iid().map(|i| i.number()), Some(iid_num));
    }

    #[test]
    fn vstopi_zero() {
        let reg = Vstopi::from_bits(0);
        assert!(reg.iid().is_none());
        assert_eq!(reg.iprio(), 0);
    }

    #[test]
    fn vstopi_zero_iid_parse() {
        let reg = Vstopi::from_bits(0x01);
        assert_eq!(reg.iid().map(|iid| iid.number()), Some(0));
        assert_eq!(reg.iprio(), 1);
    }
}
