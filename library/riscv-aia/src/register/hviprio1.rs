//! Hypervisor VS-level interrupt priority 1 (hviprio1)

#[cfg(target_pointer_width = "32")]
riscv::read_write_csr! {
    /// Hypervisor VS-level interrupt priority 1.
    Hviprio1: 0x646,
    mask: 0xFF00_FF00,
}

#[cfg(target_pointer_width = "64")]
riscv::read_write_csr! {
    /// Hypervisor VS-level interrupt priority 1.
    Hviprio1: 0x646,
    mask: 0xFFFF_FF00_FF00_FF00,
}

impl Hviprio1 {
    /// Supervisor software interrupt priority number (bits 15:8).
    #[inline]
    pub const fn ssoft(self) -> u8 {
        self.priority_at_byte_index(1)
    }

    /// Set supervisor software interrupt priority number (bits 15:8).
    #[inline]
    pub const fn set_ssoft(&mut self, value: u8) {
        self.set_priority_at_byte_index(1, value)
    }

    /// Supervisor timer interrupt priority number (bits 31:24).
    #[inline]
    pub const fn stimer(self) -> u8 {
        self.priority_at_byte_index(3)
    }

    /// Set supervisor timer interrupt priority number (bits 31:24).
    #[inline]
    pub const fn set_stimer(&mut self, value: u8) {
        self.set_priority_at_byte_index(3, value)
    }

    #[cfg(target_pointer_width = "64")]
    /// Counter overflow interrupt priority number (bits 47:40).
    #[inline]
    pub const fn counter_overflow(self) -> u8 {
        self.priority_at_byte_index(5)
    }

    #[cfg(target_pointer_width = "64")]
    /// Set counter overflow interrupt priority number (bits 47:40).
    #[inline]
    pub const fn set_counter_overflow(&mut self, value: u8) {
        self.set_priority_at_byte_index(5, value)
    }

    #[cfg(target_pointer_width = "64")]
    /// Interrupt 14 priority number (bits 55:48).
    #[inline]
    pub const fn interrupt_14(self) -> u8 {
        self.priority_at_byte_index(6)
    }

    #[cfg(target_pointer_width = "64")]
    /// Set interrupt 14 priority number (bits 55:48).
    #[inline]
    pub const fn set_interrupt_14(&mut self, value: u8) {
        self.set_priority_at_byte_index(6, value)
    }

    #[cfg(target_pointer_width = "64")]
    /// Interrupt 15 priority number (bits 63:56).
    #[inline]
    pub const fn interrupt_15(self) -> u8 {
        self.priority_at_byte_index(7)
    }

    #[cfg(target_pointer_width = "64")]
    /// Set interrupt 15 priority number (bits 63:56).
    #[inline]
    pub const fn set_interrupt_15(&mut self, value: u8) {
        self.set_priority_at_byte_index(7, value)
    }

    /// Returns the priority stored at packed-register byte index `byte_index`.
    /// Byte 0 corresponds to bits 7:0, byte 1 to bits 15:8, etc.
    /// Valid indices are 0..4 on RV32 and 0..8 on RV64.
    #[inline]
    const fn priority_at_byte_index(self, byte_index: usize) -> u8 {
        let shift = byte_index * 8;
        ((self.bits >> shift) & 0xFF) as u8
    }

    /// Sets the priority stored at packed-register byte index `byte_index`.
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
    fn hviprio1_low_set_get() {
        let reg = Hviprio1::from_bits(0x1234_5678);
        assert_eq!(reg.ssoft(), 0x56);
        assert_eq!(reg.stimer(), 0x12);
        assert_eq!(reg.priority_at_byte_index(0), 0);
        assert_eq!(reg.priority_at_byte_index(2), 0);

        let mut updated = Hviprio1::from_bits(0);
        updated.set_ssoft(0xA5);
        updated.set_stimer(0x5A);
        assert_eq!(updated.ssoft(), 0xA5);
        assert_eq!(updated.stimer(), 0x5A);
        assert_eq!(updated.bits(), 0x5A00_A500);
    }

    #[cfg(target_pointer_width = "64")]
    #[test]
    fn hviprio1_rv64_mask() {
        assert_eq!(Hviprio1::BITMASK, 0xFFFF_FF00_FF00_FF00);
    }

    #[cfg(target_pointer_width = "64")]
    #[test]
    fn hviprio1_rv64_high_set_get() {
        let reg = Hviprio1::from_bits(0x1234_5678_9ABC_DEF0);
        assert_eq!(reg.priority_at_byte_index(4), 0);
        assert_eq!(reg.counter_overflow(), 0x56);
        assert_eq!(reg.interrupt_14(), 0x34);
        assert_eq!(reg.interrupt_15(), 0x12);

        let mut updated = Hviprio1::from_bits(0);
        updated.set_counter_overflow(0xA5);
        updated.set_interrupt_14(0x5A);
        updated.set_interrupt_15(0x3C);
        assert_eq!(updated.counter_overflow(), 0xA5);
        assert_eq!(updated.interrupt_14(), 0x5A);
        assert_eq!(updated.interrupt_15(), 0x3C);
        assert_eq!(updated.bits(), 0x3C5A_A500_0000_0000);
    }

    #[cfg(target_pointer_width = "32")]
    #[test]
    fn hviprio1_rv32_mask() {
        assert_eq!(Hviprio1::BITMASK, 0xFF00_FF00);
        let reg = Hviprio1::from_bits(0x1234_5678);
        assert_eq!(reg.bits(), 0x1200_5600);
    }
}
