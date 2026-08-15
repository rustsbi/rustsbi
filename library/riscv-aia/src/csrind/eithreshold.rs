//! External interrupt threshold register.

/// Indirect CSR identifier for `eithreshold`.
pub const EITHRESHOLD: u32 = 0x72;

/// External interrupt threshold register (`eithreshold`).
///
/// This register specifies the minimum priority level for an interrupt
/// to be delivered. Interrupts with priority below this threshold are masked.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct Eithreshold(u32);

impl Eithreshold {
    /// Creates a new `Eithreshold` from a raw value.
    pub const fn from_raw(value: u32) -> Self {
        Self(value)
    }

    /// Returns the raw value of this register.
    pub const fn raw(self) -> u32 {
        self.0
    }

    /// Returns the threshold value.
    pub const fn threshold(self) -> u32 {
        self.0
    }

    /// Returns the priority threshold.
    pub const fn priority(self) -> u8 {
        (self.0 >> 24) as u8
    }
}
