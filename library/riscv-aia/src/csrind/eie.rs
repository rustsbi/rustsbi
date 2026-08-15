//! External interrupt-enable registers.

/// Indirect CSR identifier for `eie[n]`. registers (eie0 to eie63).
pub const EIE_BASE: u32 = 0xC0;

/// External interrupt-enable bits register (`eie[n]`).
///
/// Each bit represents whether the corresponding interrupt identity is enabled.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct Eie(u32);

impl Eie {
    /// Creates a new `Eie` from a raw value.
    pub const fn from_raw(value: u32) -> Self {
        Self(value)
    }

    /// Returns the raw value of this register.
    pub const fn raw(self) -> u32 {
        self.0
    }

    /// Returns whether the interrupt with the given index (0-31) is enabled.
    pub const fn is_enabled(self, index: u32) -> bool {
        if index >= 32 {
            return false;
        }
        (self.0 & (1 << index)) != 0
    }

    /// Sets the enable status for the interrupt with the given index (0-31).
    pub const fn set_enabled(mut self, index: u32, enabled: bool) -> Self {
        if index >= 32 {
            return self;
        }
        if enabled {
            self.0 |= 1 << index;
        } else {
            self.0 &= !(1 << index);
        }
        self
    }
}
