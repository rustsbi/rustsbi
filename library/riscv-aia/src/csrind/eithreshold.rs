//! External interrupt threshold register.

/// Indirect CSR identifier for `eithreshold`.
const EITHRESHOLD: usize = 0x72;

/// External interrupt threshold register (`eithreshold`).
///
/// This WLRL register controls which interrupt identities can contribute to
/// signaling an interrupt. When the value is a nonzero value `P`, interrupt
/// identities `P` and higher are excluded. A value of zero allows all enabled
/// interrupt identities to contribute.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct Eithreshold(usize);

impl Eithreshold {
    /// Creates a new `Eithreshold` from a raw value.
    pub const fn from_bits(value: usize) -> Self {
        Self(value)
    }

    /// Returns the raw value of this register.
    pub const fn bits(self) -> usize {
        self.0
    }

    /// Returns the threshold value.
    pub const fn threshold(self) -> usize {
        self.0
    }
}

impl_ind_accessors! {
    EITHRESHOLD, Eithreshold, "external interrupt threshold",
    machine => (
        "M-mode accessors for `eithreshold` register.",
        "The current hart must implement Smaia, and the caller must be permitted to access M-mode CSRs."
    ),
    supervisor => (
        "S-mode accessors for `eithreshold` register.",
        "The current hart must implement Ssaia, and the caller must be permitted to access S-mode CSRs."
    ),
    guest => (
        "VS-mode accessors for `eithreshold` register.",
        "The current hart must implement the H extension with a guest interrupt file, the caller must be permitted to access VS-mode CSRs, and `hstatus.VGEIN` must select an implemented guest interrupt file."
    ),
}
