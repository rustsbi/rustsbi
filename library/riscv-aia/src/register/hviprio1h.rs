//! Hypervisor VIPRIO1 high-half (hviprio1h) (RV32 only)

riscv::csr! {
    /// Upper 32 bits of hviprio1.
    Hviprio1h,
    0xFFFF_FF00
}
riscv::read_csr_as_rv32!(Hviprio1h, 0x656);
riscv::write_csr_as_rv32!(Hviprio1h, 0x656);

impl Hviprio1h {
    /// Counter overflow interrupt priority number (bits 47:40).
    #[inline]
    pub const fn counter_overflow(self) -> u8 {
        ((self.bits >> 8) & 0xFF) as u8
    }

    /// Set counter overflow interrupt priority number (bits 47:40).
    #[inline]
    pub const fn set_counter_overflow(&mut self, value: u8) {
        self.set_priority_at_byte_index(1, value)
    }

    /// Interrupt 14 priority number (bits 55:48).
    #[inline]
    pub const fn interrupt_14(self) -> u8 {
        ((self.bits >> 16) & 0xFF) as u8
    }

    /// Set interrupt 14 priority number (bits 55:48).
    #[inline]
    pub const fn set_interrupt_14(&mut self, value: u8) {
        self.set_priority_at_byte_index(2, value)
    }

    /// Interrupt 15 priority number (bits 63:56).
    #[inline]
    pub const fn interrupt_15(self) -> u8 {
        ((self.bits >> 24) & 0xFF) as u8
    }

    /// Set interrupt 15 priority number (bits 63:56).
    #[inline]
    pub const fn set_interrupt_15(&mut self, value: u8) {
        self.set_priority_at_byte_index(3, value)
    }

    #[inline]
    const fn set_priority_at_byte_index(&mut self, byte_index: usize, value: u8) {
        let shift = byte_index * 8;
        self.bits = (self.bits & !(0xFFusize << shift)) | ((value as usize) << shift);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hviprio1h_mask() {
        let reg = Hviprio1h::from_bits(0x1234_5678);
        assert_eq!(Hviprio1h::BITMASK, 0xFFFF_FF00);
        assert_eq!(reg.bits(), 0x1234_5600);
    }

    #[test]
    fn hviprio1h_set_get() {
        let reg = Hviprio1h::from_bits(0x1234_5678);
        assert_eq!(reg.counter_overflow(), 0x56);
        assert_eq!(reg.interrupt_14(), 0x34);
        assert_eq!(reg.interrupt_15(), 0x12);

        let mut updated = Hviprio1h::from_bits(0);
        updated.set_counter_overflow(0xA5);
        updated.set_interrupt_14(0x5A);
        updated.set_interrupt_15(0x3C);
        assert_eq!(updated.counter_overflow(), 0xA5);
        assert_eq!(updated.interrupt_14(), 0x5A);
        assert_eq!(updated.interrupt_15(), 0x3C);
        assert_eq!(updated.bits(), 0x3C5A_A500);
    }
}
