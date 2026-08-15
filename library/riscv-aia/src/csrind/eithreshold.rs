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

/// M-mode accessors for `eithreshold` register.
pub mod machine {
    use super::super::machine::{read_ind, write_ind};
    use super::{EITHRESHOLD, Eithreshold};

    /// Reads the external interrupt threshold register.
    pub fn read() -> Eithreshold {
        let bits = unsafe { read_ind(EITHRESHOLD) };
        Eithreshold::from_bits(bits)
    }

    /// Writes the external interrupt threshold register.
    pub unsafe fn write(value: Eithreshold) {
        unsafe { write_ind(EITHRESHOLD, value.bits()) }
    }
}

/// S-mode accessors for `eithreshold` register.
pub mod supervisor {
    use super::super::supervisor::{read_ind, write_ind};
    use super::{EITHRESHOLD, Eithreshold};

    /// Reads the external interrupt threshold register.
    pub fn read() -> Eithreshold {
        let bits = unsafe { read_ind(EITHRESHOLD) };
        Eithreshold::from_bits(bits)
    }

    /// Writes the external interrupt threshold register.
    pub unsafe fn write(value: Eithreshold) {
        unsafe { write_ind(EITHRESHOLD, value.bits()) }
    }
}

/// VS-mode accessors for `eithreshold` register.
pub mod guest {
    use super::super::guest::{read_ind, write_ind};
    use super::{EITHRESHOLD, Eithreshold};

    /// Reads the external interrupt threshold register.
    pub fn read() -> Eithreshold {
        let bits = unsafe { read_ind(EITHRESHOLD) };
        Eithreshold::from_bits(bits)
    }

    /// Writes the external interrupt threshold register.
    pub unsafe fn write(value: Eithreshold) {
        unsafe { write_ind(EITHRESHOLD, value.bits()) }
    }
}
