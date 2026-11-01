//! Physical memory protection registers.
//!
//! See <https://docs.riscv.org/reference/isa/priv/machine.html#_physical_memory_protection>.

/// PMP address matching mode.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum PmpRange {
    Off = 0,
    Tor = 1,
    Na4 = 2,
    Napot = 3,
}

/// PMP read, write and execute permission bits.
///
/// Some combinations are reserved by the privileged specification. Hardware
/// reads may still produce any bit pattern, so each encoding is represented.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum PmpPermission {
    None = 0,
    R = 1,
    W = 2,
    Rw = 3,
    X = 4,
    Rx = 5,
    Wx = 6,
    Rwx = 7,
}

/// One eight-bit PMP configuration field.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PmpEntry {
    pub range: PmpRange,
    pub permission: PmpPermission,
    pub locked: bool,
}

impl PmpEntry {
    /// Decodes the architecturally defined bits in a configuration field.
    pub const fn from_bits(bits: u8) -> Self {
        let range = match (bits >> 3) & 3 {
            0 => PmpRange::Off,
            1 => PmpRange::Tor,
            2 => PmpRange::Na4,
            _ => PmpRange::Napot,
        };
        let permission = match bits & 7 {
            0 => PmpPermission::None,
            1 => PmpPermission::R,
            2 => PmpPermission::W,
            3 => PmpPermission::Rw,
            4 => PmpPermission::X,
            5 => PmpPermission::Rx,
            6 => PmpPermission::Wx,
            _ => PmpPermission::Rwx,
        };
        Self {
            range,
            permission,
            locked: bits & 0x80 != 0,
        }
    }

    /// Encodes the architecturally defined bits; reserved bits are zero.
    pub const fn bits(self) -> u8 {
        (self.range as u8) << 3 | self.permission as u8 | ((self.locked as u8) << 7)
    }
}

/// PMP configuration CSR selected by its architectural register index.
pub enum PmpConfiguration<const INDEX: u8> {}

impl<const INDEX: u8> PmpConfiguration<INDEX> {
    /// CSR number; RV64 implements only even-numbered configuration CSRs.
    pub const NUMBER: u16 = {
        assert!(INDEX < 16 && (usize::BITS == 32 || INDEX.is_multiple_of(2)));
        0x3a0 + INDEX as u16
    };

    /// Reads the current hart's PMP configuration word.
    /// Access to an unimplemented CSR can trap.
    pub fn read() -> usize {
        #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
        {
            let value;
            // SAFETY: Reading the register does not change PMP state.
            unsafe {
                core::arch::asm!("csrr {}, {csr}", out(reg) value, csr = const Self::NUMBER, options(nostack))
            };
            value
        }
        #[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
        unimplemented!("CSR access requires RISC-V")
    }

    /// Writes the current hart's PMP configuration word.
    ///
    /// # Safety
    /// Execute in M-mode and maintain memory-access and trap-handler invariants.
    /// The selected CSR must be implemented and the value must obey the
    /// implementation's supported permission and range encodings.
    /// Coordinate all PMP users; locked fields may become immutable until reset.
    /// Synchronize address translation after changing PMP settings as required
    /// by the privileged architecture.
    pub unsafe fn write(value: usize) {
        #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
        // SAFETY: The caller maintains the PMP configuration invariants.
        unsafe {
            core::arch::asm!("csrw {csr}, {value}", csr = const Self::NUMBER, value = in(reg) value, options(nostack))
        }
        #[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
        {
            let _ = value;
            unimplemented!("CSR access requires RISC-V")
        }
    }

    /// Updates one configuration byte in this word.
    ///
    /// # Panics
    ///
    /// Panics if `slot` is not less than XLEN / 8.
    ///
    /// # Safety
    ///
    /// The caller must satisfy [`Self::write`]'s requirements and exclude
    /// concurrent updates between the read and write.
    pub unsafe fn set(slot: usize, entry: PmpEntry) {
        assert!(slot < core::mem::size_of::<usize>());
        let shift = slot * 8;
        let value = (Self::read() & !(0xffusize << shift)) | ((entry.bits() as usize) << shift);
        // SAFETY: The caller maintains the PMP configuration invariants.
        unsafe { Self::write(value) };
    }
}

/// PMP address CSR for architectural entry `INDEX`.
pub enum PmpAddress<const INDEX: u8> {}

impl<const INDEX: u8> PmpAddress<INDEX> {
    /// Architectural CSR number.
    pub const NUMBER: u16 = {
        assert!(INDEX < 64);
        0x3b0 + INDEX as u16
    };

    /// Reads the current hart's PMP address encoding.
    /// Access to an unimplemented CSR can trap.
    pub fn read() -> usize {
        #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
        {
            let value;
            // SAFETY: Reading the register does not change PMP state.
            unsafe {
                core::arch::asm!("csrr {}, {csr}", out(reg) value, csr = const Self::NUMBER, options(nostack))
            };
            value
        }
        #[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
        unimplemented!("CSR access requires RISC-V")
    }

    /// Writes the current hart's PMP address encoding.
    ///
    /// # Safety
    /// Execute in M-mode and maintain memory-access and trap-handler invariants.
    /// The selected CSR must be implemented.
    /// Coordinate with all PMP users and the corresponding configuration field.
    /// Synchronize address translation after changing PMP settings as required
    /// by the privileged architecture.
    pub unsafe fn write(value: usize) {
        #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
        // SAFETY: The caller maintains the PMP configuration invariants.
        unsafe {
            core::arch::asm!("csrw {csr}, {value}", csr = const Self::NUMBER, value = in(reg) value, options(nostack))
        }
        #[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
        {
            let _ = value;
            unimplemented!("CSR access requires RISC-V")
        }
    }
}
