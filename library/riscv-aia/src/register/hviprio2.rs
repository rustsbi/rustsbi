//! Hypervisor VS-level interrupt priority 2 (hviprio2)

// Defined in the specification, Section 6.3, page 70:
//
// > hviprio2:
// > bits 7:0 Priority number for interrupt 16
// > bits 15:8 Priority number for interrupt 17
// > bits 23:16 Priority number for interrupt 18
// > bits 31:24 Priority number for interrupt 19
// > bits 39:32 Priority number for interrupt 20
// > bits 47:40 Priority number for interrupt 21
// > bits 55:48 Priority number for interrupt 22
// > bits 63:56 Priority number for interrupt 23

riscv::read_write_csr! {
    /// Hypervisor VS-level interrupt priority 2.
    Hviprio2: 0x647,
    mask: usize::MAX,
}

impl Hviprio2 {
    /// Interrupt 16 priority number (bits 7:0).
    #[inline]
    pub const fn interrupt_16(self) -> u8 {
        self.prio_byte(0)
    }

    /// Interrupt 17 priority number (bits 15:8).
    #[inline]
    pub const fn interrupt_17(self) -> u8 {
        self.prio_byte(1)
    }

    /// Interrupt 18 priority number (bits 23:16).
    #[inline]
    pub const fn interrupt_18(self) -> u8 {
        self.prio_byte(2)
    }

    /// Interrupt 19 priority number (bits 31:24).
    #[inline]
    pub const fn interrupt_19(self) -> u8 {
        self.prio_byte(3)
    }

    #[cfg(not(target_pointer_width = "32"))]
    /// Interrupt 20 priority number (bits 39:32).
    #[inline]
    pub const fn interrupt_20(self) -> u8 {
        self.prio_byte(4)
    }

    #[cfg(not(target_pointer_width = "32"))]
    /// Interrupt 21 priority number (bits 47:40).
    #[inline]
    pub const fn interrupt_21(self) -> u8 {
        self.prio_byte(5)
    }

    #[cfg(not(target_pointer_width = "32"))]
    /// Interrupt 22 priority number (bits 55:48).
    #[inline]
    pub const fn interrupt_22(self) -> u8 {
        self.prio_byte(6)
    }

    #[cfg(not(target_pointer_width = "32"))]
    /// Interrupt 23 priority number (bits 63:56).
    #[inline]
    pub const fn interrupt_23(self) -> u8 {
        self.prio_byte(7)
    }

    /// Returns the priority byte at byte index `i`.
    /// Byte 0 corresponds to bits 7:0, byte 1 to bits 15:8, etc.
    /// Valid indices are 0..4 on RV32 and 0..8 on RV64.
    #[inline]
    const fn prio_byte(self, i: usize) -> u8 {
        let shift = i * 8;
        ((self.bits >> shift) & 0xFF) as u8
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hviprio2_low_fields() {
        let reg = Hviprio2::from_bits(0x1234_5678);
        assert_eq!(reg.interrupt_16(), 0x78);
        assert_eq!(reg.interrupt_17(), 0x56);
        assert_eq!(reg.interrupt_18(), 0x34);
        assert_eq!(reg.interrupt_19(), 0x12);
    }

    #[cfg(not(target_pointer_width = "32"))]
    #[test]
    fn hviprio2_rv64_high_fields() {
        assert_eq!(Hviprio2::BITMASK, usize::MAX);
        let reg = Hviprio2::from_bits(0x1234_5678_9ABC_DEF0);
        assert_eq!(reg.bits(), 0x1234_5678_9ABC_DEF0);
        assert_eq!(reg.interrupt_20(), 0x78);
        assert_eq!(reg.interrupt_21(), 0x56);
        assert_eq!(reg.interrupt_22(), 0x34);
        assert_eq!(reg.interrupt_23(), 0x12);
    }

    #[cfg(target_pointer_width = "32")]
    #[test]
    fn hviprio2_rv32_mask() {
        assert_eq!(Hviprio2::BITMASK, usize::MAX);
        let reg = Hviprio2::from_bits(0x1234_5678);
        assert_eq!(reg.bits(), 0x1234_5678);
    }
}
