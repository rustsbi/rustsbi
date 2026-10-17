//! Machine interrupt-file indirect register identities.

/// An IMSIC indirect register, distinct from the `miselect` and `mireg` CSRs.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ImsicRegister {
    /// Interrupt delivery enable.
    Delivery,
    /// Interrupt priority threshold.
    Threshold,
    /// A pending-bit array register.
    Pending(u8),
    /// An enable-bit array register.
    Enable(u8),
}

impl ImsicRegister {
    /// Returns the architectural selector, rejecting absent bitmap indices.
    /// RV64 implements only even-numbered pending and enable registers.
    pub const fn selector(self) -> Option<usize> {
        let (base, index) = match self {
            Self::Delivery => return Some(0x70),
            Self::Threshold => return Some(0x72),
            Self::Pending(index) => (0x80, index),
            Self::Enable(index) => (0xc0, index),
        };
        if index >= 64 || (cfg!(target_pointer_width = "64") && index % 2 != 0) {
            None
        } else {
            Some(base + index as usize)
        }
    }
}
