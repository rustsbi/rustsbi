//! External interrupt-pending bits registers.

/// Indirect CSR base identifier for `eip[n]` registers (eip0 to eip63).
pub const EIP_BASE: usize = 0x80;

/// External interrupt-pending bits register (`eip[n]`).
///
/// Each bit represents whether the corresponding interrupt identity is pending.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct Eip(u32);

impl Eip {
    /// Creates a new `Eip` from a raw value.
    pub const fn from_bits(value: u32) -> Self {
        Self(value)
    }

    /// Returns the raw value of this register.
    pub const fn bits(self) -> u32 {
        self.0
    }

    /// Returns whether the interrupt with the given index (0-31) is pending.
    pub const fn is_pending(self, index: u32) -> bool {
        if index >= 32 {
            return false;
        }
        (self.0 & (1 << index)) != 0
    }

    /// Sets the pending status for the interrupt with the given index (0-31).
    pub const fn set_pending(mut self, index: u32, pending: bool) -> Self {
        if index >= 32 {
            return self;
        }
        if pending {
            self.0 |= 1 << index;
        } else {
            self.0 &= !(1 << index);
        }
        self
    }
}
