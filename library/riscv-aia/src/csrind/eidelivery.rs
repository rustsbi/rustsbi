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
    pub const DISABLED: Self = Self(0);

    /// Interrupt delivery from the interrupt file is enabled.
    pub const ENABLED: Self = Self(1);

    /// Interrupt delivery from a PLIC or APLIC is enabled (optional).
    pub const PLIC_APLIC_ENABLED: Self = Self(0x40000000);

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

impl_ind_accessors! {
    EIDELIVERY, Eidelivery, "external interrupt delivery enable",
    machine => (
        "M-mode accessors for `eidelivery` register.",
        "The current hart must implement Smaia, and the caller must be permitted to access M-mode CSRs."
    ),
    supervisor => (
        "S-mode accessors for `eidelivery` register.",
        "The current hart must implement Ssaia, and the caller must be permitted to access S-mode CSRs."
    ),
    guest => (
        "VS-mode accessors for `eidelivery` register.",
        "The current hart must implement the H extension with a guest interrupt file, the caller must be permitted to access VS-mode CSRs, and `hstatus.VGEIN` must select an implemented guest interrupt file."
    ),
}

#[cfg(test)]
mod tests {
    use super::Eidelivery;

    #[test]
    fn defined_values_and_predicates() {
        let disabled = Eidelivery::from_bits(0);
        assert_eq!(disabled, Eidelivery::DISABLED);
        assert_eq!(disabled.bits(), 0);
        assert!(disabled.is_disabled());
        assert!(!disabled.is_enabled());
        assert!(!disabled.is_plic_aplic_enabled());

        let enabled = Eidelivery::from_bits(1);
        assert_eq!(enabled, Eidelivery::ENABLED);
        assert_eq!(enabled.bits(), 1);
        assert!(!enabled.is_disabled());
        assert!(enabled.is_enabled());
        assert!(!enabled.is_plic_aplic_enabled());

        let plic_aplic = Eidelivery::from_bits(0x40000000);
        assert_eq!(plic_aplic, Eidelivery::PLIC_APLIC_ENABLED);
        assert_eq!(plic_aplic.bits(), 0x40000000);
        assert!(!plic_aplic.is_disabled());
        assert!(!plic_aplic.is_enabled());
        assert!(plic_aplic.is_plic_aplic_enabled());

        let other = Eidelivery::from_bits(2);
        assert!(!other.is_disabled());
        assert!(!other.is_enabled());
        assert!(!other.is_plic_aplic_enabled());
    }
}
