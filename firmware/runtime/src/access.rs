//! Optional platform access handler for S-mode load/store access faults.
//!
//! Runtime keeps instruction semantics and exposes only the physical address,
//! access width, and raw transferred value to the platform.

use spin::Once;

/// Per-access callbacks installed once during boot by the platform firmware.
///
/// The callbacks are indirect calls that avoid a trait object on the
/// access-fault path.
pub struct AccessHandlers {
    /// Emulates a `width`-byte load from physical `addr`, returning the raw
    /// unsigned value, or `None` when the platform does not own the address.
    pub load: fn(addr: usize, width: usize) -> Option<usize>,
    /// Emulates a `width`-byte store of `value` to physical `addr`, returning
    /// whether the platform owned the address.
    pub store: fn(addr: usize, width: usize, value: usize) -> bool,
}

static HANDLERS: Once<Option<AccessHandlers>> = Once::new();

/// Publishes the optional platform access callbacks once during boot.
pub fn install(handlers: Option<AccessHandlers>) {
    HANDLERS.call_once(|| handlers);
}

/// Returns the published platform access callbacks.
pub(crate) fn get() -> Option<&'static AccessHandlers> {
    HANDLERS.get().and_then(|h| h.as_ref())
}
