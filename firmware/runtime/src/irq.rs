//! Local machine-interrupt exclusion for Runtime hardware operations.

use core::marker::PhantomData;
use csr::{Clear, Set, Write, standard::isa::privileged::machine::Mstatus};

/// Proof that local machine interrupts remain disabled within a scope.
///
/// This context cannot be transferred to another hart or retained after
/// its Runtime interrupt exclusion scope ends.
/// It does not provide remote mutual exclusion.
pub struct LocalIrqDisabled {
    _local: PhantomData<*mut ()>,
}

impl LocalIrqDisabled {
    /// Records exclusion already established by a Runtime entry.
    ///
    /// # Safety
    ///
    /// Keep MIE clear and stay on this hart while the context is in use.
    pub(crate) unsafe fn assume_disabled() -> Self {
        Self {
            _local: PhantomData,
        }
    }
}

/// Runs `f` with local machine interrupts disabled, then restores their state.
///
/// Nested calls leave interrupts masked until the outermost scope that
/// disabled them returns. The borrowed IRQ context cannot escape `f`.
pub(crate) fn with_local_disabled<R>(f: impl FnOnce(&LocalIrqDisabled) -> R) -> R {
    struct Restore {
        was_enabled: bool,
    }
    impl Drop for Restore {
        fn drop(&mut self) {
            if self.was_enabled {
                // SAFETY: This scope masked MIE and is ending on the same hart.
                unsafe { Mstatus::write(Set(Mstatus::MIE)) };
            }
        }
    }
    // SAFETY: Clearing MIE atomically preserves other bits and returns prior state.
    let previous = unsafe { Mstatus::write(Clear(Mstatus::MIE)) };
    let restore = Restore {
        was_enabled: previous.get(Mstatus::MIE),
    };
    let result = f(&LocalIrqDisabled {
        _local: PhantomData,
    });
    drop(restore);
    result
}
