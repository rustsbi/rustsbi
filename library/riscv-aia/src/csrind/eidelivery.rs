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

macro_rules! impl_eidelivery_accessors {
    ($($mode:ident => ($doc:literal, $safety:literal)),+ $(,)?) => {
        $(
            #[doc = $doc]
            pub mod $mode {
                use super::super::$mode::{read_ind, write_ind};
                use super::{EIDELIVERY, Eidelivery};

                /// Reads the external interrupt delivery enable register.
                pub fn read() -> Eidelivery {
                    let bits = unsafe { read_ind(EIDELIVERY) };
                    Eidelivery::from_bits(bits)
                }

                /// Writes the external interrupt delivery enable register.
                ///
                /// # Safety
                ///
                #[doc = $safety]
                pub unsafe fn write(value: Eidelivery) {
                    unsafe { write_ind(EIDELIVERY, value.bits()) }
                }
            }
        )+
    };
}

impl_eidelivery_accessors! {
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
