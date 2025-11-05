//! Hypervisor VS-level interrupt priority 2 (hviprio2)

riscv::read_write_csr! {
    /// Hypervisor VS-level interrupt priority 2.
    Hviprio2: 0x647,
    mask: 0xFFFF_FFFF_FFFF_FFFF,
}

impl Hviprio2 {
    /// Returns the priority byte at byte index `i` (0..7).
    #[inline]
    pub const fn prio_byte(self, i: usize) -> u8 {
        let shift = i * 8;
        ((self.bits >> shift) & 0xFF) as u8
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::register::hviprio1::Hviprio1;

    #[test]
    fn hviprio_prio_bytes() {
        // build a 64-bit value with known bytes: 0x00..07
        let mut val: usize = 0;
        for i in 0..8 {
            val |= (i as usize & 0xFF) << (i * 8);
        }
        let r1 = Hviprio1::from_bits(val);
        let r2 = Hviprio2::from_bits(val);
        for i in 0..8 {
            assert_eq!(r1.prio_byte(i), i as u8);
            assert_eq!(r2.prio_byte(i), i as u8);
        }
    }
}
