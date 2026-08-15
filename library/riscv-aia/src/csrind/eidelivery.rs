//! External interrupt delivery enable register.

/// Indirect CSR identifier for `eidelivery`.
pub const EIDELIVERY: u32 = 0x70;

/// External interrupt delivery enable register (`eidelivery`).
///
/// This register controls whether interrupts from this interrupt file are
/// delivered from the IMSIC to the attached hart.
///
/// *NOTE:* Guest interrupt files do not support value 0x40000000 for `eidelivery`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct Eidelivery(u32);

impl Eidelivery {
    /// Interrupt delivery is disabled.
    const DISABLED: Self = Self(0);

    /// Interrupt delivery from the interrupt file is enabled.
    const ENABLED: Self = Self(1);

    /// Interrupt delivery from a PLIC or APLIC is enabled (optional).
    pub const PLIC_APLIC_ENABLED: Self = Self(0x40000000);

    /// Creates a new `Eidelivery` from a raw value.
    pub const fn from_raw(value: u32) -> Self {
        Self(value)
    }

    /// Returns the raw value of this register.
    pub const fn raw(self) -> u32 {
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
