//! Machine-level top interrupt register.
//!
//! CSR `mtopi` reports the highest-priority interrupt that is pending and enabled for machine level.
use crate::iid::MajorIid;

riscv::read_only_csr! {
    /// Machine top interrupt register.
    Mtopi: 0xFB0,
    mask: 0x0FFF_00FF,
}

impl Mtopi {
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
    fn mtopi_parse() {
        // iid = 0x123, iprio = 0x7
        let iid_num: u16 = 0x123;
        let iprio: u8 = 0x7;
        let bits: usize = ((iid_num as usize) << 16) | (iprio as usize);
        let reg = Mtopi::from_bits(bits);
        assert_eq!(reg.iprio(), iprio);
        assert_eq!(reg.iid().map(|i| i.number()), Some(iid_num));
    }

    #[test]
    fn mtopi_zero() {
        // iid = 0 -> none
        let reg = Mtopi::from_bits(0);
        assert!(reg.iid().is_none());
        assert_eq!(reg.iprio(), 0);
    }

    #[test]
    fn mtopi_bounds() {
        // iid = 1, iprio = 0xFF
        let bits: usize = (1usize << 16) | 0xFF;
        let reg2 = Mtopi::from_bits(bits);
        assert_eq!(reg2.iid().map(|i| i.number()), Some(1));
        assert_eq!(reg2.iprio(), 0xFF);

        let max = Mtopi::from_bits(0x0FFF_0000);
        assert_eq!(max.iid().map(|i| i.number()), Some(0x0FFF));
    }

    #[test]
    fn mtopi_zero_iid_parse() {
        let reg = Mtopi::from_bits(0x01);
        assert_eq!(reg.iid().map(|iid| iid.number()), Some(0));
        assert_eq!(reg.iprio(), 1);
    }
}
