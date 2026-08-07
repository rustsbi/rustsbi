//! Hypervisor VIPRIO2 high-half (hviprio2h) (RV32 only)

riscv::csr! {
    /// Upper 32 bits of hviprio2.
    Hviprio2h,
    0xFFFF_FFFF
}
riscv::read_csr_as_rv32!(Hviprio2h, 0x657);
riscv::write_csr_as_rv32!(Hviprio2h, 0x657);

impl Hviprio2h {
    /// Interrupt 20 priority number (bits 39:32).
    #[inline]
    pub const fn interrupt_20(self) -> u8 {
        (self.bits & 0xFF) as u8
    }

    /// Set interrupt 20 priority number (bits 39:32).
    #[inline]
    pub const fn set_interrupt_20(&mut self, value: u8) {
        self.set_prio_byte(0, value)
    }

    /// Interrupt 21 priority number (bits 47:40).
    #[inline]
    pub const fn interrupt_21(self) -> u8 {
        ((self.bits >> 8) & 0xFF) as u8
    }

    /// Set interrupt 21 priority number (bits 47:40).
    #[inline]
    pub const fn set_interrupt_21(&mut self, value: u8) {
        self.set_prio_byte(1, value)
    }

    /// Interrupt 22 priority number (bits 55:48).
    #[inline]
    pub const fn interrupt_22(self) -> u8 {
        ((self.bits >> 16) & 0xFF) as u8
    }

    /// Set interrupt 22 priority number (bits 55:48).
    #[inline]
    pub const fn set_interrupt_22(&mut self, value: u8) {
        self.set_prio_byte(2, value)
    }

    /// Interrupt 23 priority number (bits 63:56).
    #[inline]
    pub const fn interrupt_23(self) -> u8 {
        ((self.bits >> 24) & 0xFF) as u8
    }

    /// Set interrupt 23 priority number (bits 63:56).
    #[inline]
    pub const fn set_interrupt_23(&mut self, value: u8) {
        self.set_prio_byte(3, value)
    }

    #[inline]
    const fn set_prio_byte(&mut self, i: usize, value: u8) {
        let shift = i * 8;
        self.bits = (self.bits & !(0xFFusize << shift)) | ((value as usize) << shift);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hviprio2h_fields() {
        let reg = Hviprio2h::from_bits(0x1234_5678);
        assert_eq!(Hviprio2h::BITMASK, 0xFFFF_FFFF);
        assert_eq!(reg.bits(), 0x1234_5678);
        assert_eq!(reg.interrupt_20(), 0x78);
        assert_eq!(reg.interrupt_21(), 0x56);
        assert_eq!(reg.interrupt_22(), 0x34);
        assert_eq!(reg.interrupt_23(), 0x12);

        let mut updated = Hviprio2h::from_bits(0);
        updated.set_interrupt_20(0xA1);
        updated.set_interrupt_21(0xB2);
        updated.set_interrupt_22(0xC3);
        updated.set_interrupt_23(0xD4);
        assert_eq!(updated.interrupt_20(), 0xA1);
        assert_eq!(updated.interrupt_21(), 0xB2);
        assert_eq!(updated.interrupt_22(), 0xC3);
        assert_eq!(updated.interrupt_23(), 0xD4);
        assert_eq!(updated.bits(), 0xD4C3_B2A1);
    }
}
