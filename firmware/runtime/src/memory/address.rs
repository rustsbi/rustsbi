//! Physical address values.

/// An address in the physical address space.
///
/// The current representation supports platforms whose physical-address
/// width does not exceed the native pointer width (`usize`, which equals XLEN
/// on RISC-V). Wider physical addresses, such as a 34-bit physical address on
/// RV32, require a different representation and are not supported.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct PhysAddr(usize);

impl PhysAddr {
    /// Creates a physical-address value.
    #[inline]
    pub const fn new(value: usize) -> Self {
        Self(value)
    }

    /// Returns the address as a native integer.
    #[inline]
    pub const fn as_usize(self) -> usize {
        self.0
    }

    /// Adds a byte offset without wrapping.
    #[inline]
    pub const fn checked_add(self, offset: usize) -> Option<Self> {
        match self.0.checked_add(offset) {
            Some(value) => Some(Self(value)),
            None => None,
        }
    }
}
