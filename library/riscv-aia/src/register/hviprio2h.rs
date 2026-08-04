//! Hypervisor VIPRIO2 high-half (hviprio2h) (RV32 only)

riscv::read_write_csr! {
    /// Upper 32 bits of hviprio2.
    Hviprio2h: 0x657,
    mask: 0xFFFF_FFFF,
}

impl Hviprio2h {
    /// Interrupt 20 priority number (bits 39:32).
    #[inline]
    pub const fn interrupt_20(self) -> u8 {
        (self.bits & 0xFF) as u8
    }

    /// Interrupt 21 priority number (bits 47:40).
    #[inline]
    pub const fn interrupt_21(self) -> u8 {
        ((self.bits >> 8) & 0xFF) as u8
    }

    /// Interrupt 22 priority number (bits 55:48).
    #[inline]
    pub const fn interrupt_22(self) -> u8 {
        ((self.bits >> 16) & 0xFF) as u8
    }

    /// Interrupt 23 priority number (bits 63:56).
    #[inline]
    pub const fn interrupt_23(self) -> u8 {
        ((self.bits >> 24) & 0xFF) as u8
    }

    /// Returns the raw upper 32 bits of `hviprio2`.
    #[inline]
    pub const fn raw(self) -> usize {
        self.bits
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hviprio2h_fields() {
        let reg = Hviprio2h::from_bits(0x1234_5678);
        assert_eq!(Hviprio2h::BITMASK, 0xFFFF_FFFF);
        assert_eq!(reg.raw(), 0x1234_5678);
        assert_eq!(reg.interrupt_20(), 0x78);
        assert_eq!(reg.interrupt_21(), 0x56);
        assert_eq!(reg.interrupt_22(), 0x34);
        assert_eq!(reg.interrupt_23(), 0x12);
    }
}
