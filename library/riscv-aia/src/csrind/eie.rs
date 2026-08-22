//! External interrupt-enable registers.

/// Indirect CSR base identifier for `eie[n]` registers (eie0 to eie63).
pub const EIE_BASE: usize = 0xC0;

/// Number of register identifiers in the `eie` array.
const EIE_COUNT: usize = 64;

/// External interrupt-enable bits register (`eie[n]`).
///
/// Each bit represents whether the corresponding interrupt identity is enabled.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct Eie(usize);

impl Eie {
    /// Creates a new `Eie` from a raw value.
    pub const fn from_bits(value: usize) -> Self {
        Self(value)
    }

    /// Returns the raw value of this register.
    pub const fn bits(self) -> usize {
        self.0
    }

    /// Returns whether the interrupt at the given bit index is enabled.
    pub const fn is_enabled(self, index: u32) -> bool {
        if index >= usize::BITS {
            return false;
        }
        (self.0 & (1 << index)) != 0
    }

    /// Sets the enable status for the interrupt at the given bit index.
    pub const fn set_enabled(mut self, index: u32, enabled: bool) -> Self {
        if index >= usize::BITS {
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

/// Returns the indirect CSR identifier for `eie[index]`.
const fn eie_id(index: usize) -> usize {
    assert!(index < EIE_COUNT, "eie register index out of range");
    assert!(
        usize::BITS == 32 || index & 1 == 0,
        "odd-numbered eie registers do not exist on RV64"
    );
    EIE_BASE + index
}

impl_ind_accessors! {
    eie_id(index), Eie, "external interrupt-enable",
    machine => (
        "M-mode accessors for `eie` registers.",
        "The current hart must implement Smaia, and the caller must be permitted to access M-mode CSRs."
    ),
    supervisor => (
        "S-mode accessors for `eie` registers.",
        "The current hart must implement Ssaia, and the caller must be permitted to access S-mode CSRs."
    ),
    guest => (
        "VS-mode accessors for `eie` registers.",
        "The current hart must implement the H extension with a guest interrupt file, the caller must be permitted to access VS-mode CSRs, and `hstatus.VGEIN` must select an implemented guest interrupt file."
    ),
}

#[cfg(test)]
mod tests {
    use super::{EIE_BASE, Eie, eie_id};

    #[test]
    fn value_uses_xlen_bits() {
        let value = Eie::from_bits(usize::MAX);

        assert_eq!(value.bits(), usize::MAX);
        assert!(value.is_enabled(usize::BITS - 1));
    }

    #[test]
    fn selector_uses_array_index() {
        assert_eq!(eie_id(0), EIE_BASE);
        assert_eq!(eie_id(2), EIE_BASE + 2);
    }

    #[test]
    #[should_panic(expected = "eie register index out of range")]
    fn selector_rejects_out_of_range_index() {
        eie_id(64);
    }

    #[cfg(target_pointer_width = "64")]
    #[test]
    #[should_panic(expected = "odd-numbered eie registers do not exist on RV64")]
    fn selector_rejects_odd_index_on_rv64() {
        eie_id(1);
    }
}
