//! Embedded storage that becomes writable by the next-stage software.

use core::cell::UnsafeCell;

use super::PhysAddr;

/// An embedded image that firmware passes to the next stage.
///
/// The bytes use [`UnsafeCell`] because the next stage may modify them. The
/// firmware can obtain only their address and size, so it cannot retain a Rust
/// reference that conflicts with those later writes.
#[repr(C, align(4))]
pub struct HandoffBuffer<const N: usize> {
    bytes: UnsafeCell<[u8; N]>,
}

impl<const N: usize> HandoffBuffer<N> {
    /// Creates a handoff buffer containing `bytes`.
    pub const fn new(bytes: [u8; N]) -> Self {
        Self {
            bytes: UnsafeCell::new(bytes),
        }
    }

    /// Returns the physical address passed to the next stage.
    #[inline]
    pub fn address(&self) -> PhysAddr {
        PhysAddr::new(self.bytes.get().cast::<u8>() as usize)
    }

    /// Returns the buffer size in bytes.
    #[inline]
    pub const fn size(&self) -> usize {
        N
    }
}

// SAFETY: the safe interface never exposes a Rust reference to `bytes` and
// never reads or writes them. It only reports the storage address and size.
unsafe impl<const N: usize> Sync for HandoffBuffer<N> {}
