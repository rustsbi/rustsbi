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
        let shift = (i as usize) * 8;
        ((self.bits >> shift) & 0xFF) as u8
    }
}
