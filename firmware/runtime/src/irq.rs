//! Local machine-interrupt exclusion for Runtime hardware operations.

use core::marker::PhantomData;
use csr::machine;

/// Proof that local machine interrupts remain disabled within a scope.
///
/// This context cannot be transferred to another hart or retained after
/// its Runtime interrupt exclusion scope ends.
/// It does not provide remote mutual exclusion.
pub(crate) struct LocalIrqDisabled {
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
/// The restoration guard stays private, so nested scopes cannot restore IRQs
/// out of order.
/// References borrowed through the context cannot leave `f`.
pub(crate) fn with_local_disabled<R>(f: impl FnOnce(&LocalIrqDisabled) -> R) -> R {
    struct Restore {
        previous: machine::Interrupt,
    }
    impl Drop for Restore {
        fn drop(&mut self) {
            if self.previous == machine::Interrupt::Enabled {
                // SAFETY: This scope masked MIE and is ending on the same hart.
                unsafe { machine::Status::set_bit::<machine::MIE>() };
            }
        }
    }
    let previous = machine::Status::read().read_field::<machine::MIE>();
    if previous == machine::Interrupt::Enabled {
        // SAFETY: M-mode interrupt entry restores the interrupted MIE state.
        // Mask before invoking f; already-disabled callers need no CSR write.
        unsafe { machine::Status::clear_bit::<machine::MIE>() };
    }
    let restore = Restore { previous };
    let result = f(&LocalIrqDisabled {
        _local: PhantomData,
    });
    drop(restore);
    result
}
