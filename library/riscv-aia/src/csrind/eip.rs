//! External interrupt-pending bits registers.

/// Indirect CSR base identifier for `eip[n]` registers (eip0 to eip63).
pub const EIP_BASE: usize = 0x80;

/// Number of register identifiers in the `eip` array.
const EIP_COUNT: usize = 64;

/// External interrupt-pending bits register (`eip[n]`).
///
/// Each bit represents whether the corresponding interrupt identity is pending.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct Eip(usize);

impl Eip {
    /// Creates a new `Eip` from a raw value.
    pub const fn from_bits(value: usize) -> Self {
        Self(value)
    }

    /// Returns the raw value of this register.
    pub const fn bits(self) -> usize {
        self.0
    }

    /// Returns whether the interrupt at the given bit index is pending.
    pub const fn is_pending(self, index: u32) -> bool {
        if index >= usize::BITS {
            return false;
        }
        (self.0 & (1 << index)) != 0
    }

    /// Sets the pending status for the interrupt at the given bit index.
    pub const fn set_pending(mut self, index: u32, pending: bool) -> Self {
        if index >= usize::BITS {
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

/// Returns the indirect CSR identifier for `eip[index]`.
const fn eip_id(index: usize) -> usize {
    assert!(index < EIP_COUNT, "eip register index out of range");
    assert!(
        usize::BITS == 32 || index & 1 == 0,
        "odd-numbered eip registers do not exist on RV64"
    );
    EIP_BASE + index
}

impl_ind_accessors! {
    eip_id(index), Eip, "external interrupt-pending",
    machine => (
        "M-mode accessors for `eip` registers.",
        "The current hart must implement Smaia, and the caller must be permitted to access M-mode CSRs."
    ),
    supervisor => (
        "S-mode accessors for `eip` registers.",
        "The current hart must implement Ssaia, and the caller must be permitted to access S-mode CSRs."
    ),
    guest => (
        "VS-mode accessors for `eip` registers.",
        "The current hart must implement the H extension with a guest interrupt file, the caller must be permitted to access VS-mode CSRs, and `hstatus.VGEIN` must select an implemented guest interrupt file."
    ),
}

#[cfg(test)]
mod tests {
    use super::{EIP_BASE, Eip, eip_id};

    #[test]
    fn value_uses_xlen_bits() {
        let value = Eip::from_bits(usize::MAX);

        assert_eq!(value.bits(), usize::MAX);
        assert!(value.is_pending(usize::BITS - 1));
    }

    #[test]
    fn selector_uses_array_index() {
        assert_eq!(eip_id(0), EIP_BASE);
        assert_eq!(eip_id(2), EIP_BASE + 2);
    }

    #[test]
    #[should_panic(expected = "eip register index out of range")]
    fn selector_rejects_out_of_range_index() {
        eip_id(64);
    }

    #[cfg(target_pointer_width = "64")]
    #[test]
    #[should_panic(expected = "odd-numbered eip registers do not exist on RV64")]
    fn selector_rejects_odd_index_on_rv64() {
        eip_id(1);
    }
}
