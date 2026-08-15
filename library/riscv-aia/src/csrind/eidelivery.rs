//! External interrupt delivery enable register.

/// Indirect CSR identifier for `eidelivery`.
const EIDELIVERY: usize = 0x70;

/// External interrupt delivery enable register (`eidelivery`).
///
/// This register controls whether interrupts from this interrupt file are
/// delivered from the IMSIC to the attached hart.
///
/// *NOTE:* Guest interrupt files do not support value 0x40000000 for `eidelivery`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct Eidelivery(usize);

impl Eidelivery {
    /// Interrupt delivery is disabled.
    const DISABLED: Self = Self(0);

    /// Interrupt delivery from the interrupt file is enabled.
    const ENABLED: Self = Self(1);

    /// Interrupt delivery from a PLIC or APLIC is enabled (optional).
    const PLIC_APLIC_ENABLED: Self = Self(0x40000000);

    /// Creates a new `Eidelivery` from a raw value.
    pub const fn from_bits(value: usize) -> Self {
        Self(value)
    }

    /// Returns the raw value of this register.
    pub const fn bits(self) -> usize {
        self.0
    }

    /// Returns whether interrupt delivery is disabled.
    pub const fn is_disabled(self) -> bool {
        self.0 == Self::DISABLED.0
    }

    /// Returns whether interrupt delivery from the interrupt file is enabled.
    pub const fn is_enabled(self) -> bool {
        self.0 == Self::ENABLED.0
    }

    /// Returns whether interrupt delivery from a PLIC or APLIC is enabled.
    pub const fn is_plic_aplic_enabled(self) -> bool {
        self.0 == Self::PLIC_APLIC_ENABLED.0
    }
}

/// M-mode accessors for `eidelivery` register.
pub mod machine {
    use super::super::machine::{read_ind, write_ind};
    use super::{EIDELIVERY, Eidelivery};

    /// Reads the external interrupt delivery enable register.
    pub fn read() -> Eidelivery {
        let bits = unsafe { read_ind(EIDELIVERY) };
        Eidelivery::from_bits(bits)
    }

    /// Writes the external interrupt delivery enable register.
    pub unsafe fn write(value: Eidelivery) {
        unsafe { write_ind(EIDELIVERY, value.bits()) }
    }
}

/// S-mode accessors for `eidelivery` register.
pub mod supervisor {
    use super::super::supervisor::{read_ind, write_ind};
    use super::{EIDELIVERY, Eidelivery};

    /// Reads the external interrupt delivery enable register.
    pub fn read() -> Eidelivery {
        let bits = unsafe { read_ind(EIDELIVERY) };
        Eidelivery::from_bits(bits)
    }

    /// Writes the external interrupt delivery enable register.
    pub unsafe fn write(value: Eidelivery) {
        unsafe { write_ind(EIDELIVERY, value.bits()) }
    }
}

/// VS-mode accessors for `eidelivery` register.
pub mod guest {
    use super::super::guest::{read_ind, write_ind};
    use super::{EIDELIVERY, Eidelivery};

    /// Reads the external interrupt delivery enable register.
    pub fn read() -> Eidelivery {
        let bits = unsafe { read_ind(EIDELIVERY) };
        Eidelivery::from_bits(bits)
    }

    /// Writes the external interrupt delivery enable register.
    pub unsafe fn write(value: Eidelivery) {
        unsafe { write_ind(EIDELIVERY, value.bits()) }
    }
}
