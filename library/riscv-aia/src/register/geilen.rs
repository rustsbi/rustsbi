//! Guest external interrupt line number (geilen)

riscv::read_write_csr! {
    /// Guest external interrupt line number.
    Geilen: 0x312,
    mask: 0xFFFF_FFFF_FFFF_FFFF,
}

impl Geilen {
    /// Get the number of guest interrupt files per hart.
    #[inline]
    pub const fn guest_files_per_hart(self) -> u32 {
        (self.bits & 0x3F) as u32
    }

    /// Set the number of guest interrupt files per hart (0-63).
    #[inline]
    pub const fn set_guest_files_per_hart(mut self, count: usize) -> Self {
        self.bits = (self.bits & !0x3F) | (count & 0x3F);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn geilen_guest_files() {
        let reg = Geilen::from_bits(0x2A);
        assert_eq!(reg.guest_files_per_hart(), 42);

        let modified = reg.set_guest_files_per_hart(63);
        assert_eq!(modified.guest_files_per_hart(), 63);

        let zero = reg.set_guest_files_per_hart(0);
        assert_eq!(zero.guest_files_per_hart(), 0);
    }
}
