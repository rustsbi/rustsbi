//! Hypervisor VS-level interrupt priority 1 (hviprio1)

riscv::read_write_csr! {
    /// Hypervisor VS-level interrupt priority 1.
    Hviprio1: 0x646,
    mask: 0xFFFF_FFFF_FFFF_FFFF,
}

impl Hviprio1 {
    /// Returns the priority byte at byte index `i` (0..7).
    /// Byte 0 corresponds to bits 7:0, byte 1 to bits 15:8, etc.
    #[inline]
    pub const fn prio_byte(self, i: usize) -> u8 {
        let shift = (i as usize) * 8;
        ((self.bits >> shift) & 0xFF) as u8
    }
}
