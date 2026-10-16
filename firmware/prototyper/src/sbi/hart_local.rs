//! Hart-local policy state: the per-hart bookkeeping consumed by the SBI
//! extension layer.
//!
//! This storage is policy state, deliberately decoupled from the
//! Runtime-owned trap stacks: the Runtime owns stack memory; the policy
//! firmware owns its own context records. Slots are initialized once during
//! boot, then their mutable fields are accessed through atomics or locks.

#![forbid(unsafe_code)]

use alloc::boxed::Box;
use core::sync::atomic::AtomicU8;
use runtime::hart::HartId;
use spin::{Mutex, Once};

use super::features::HartFeatures;
use super::rfence::queue::{LocalRFenceCell, RFenceCell, RemoteRFenceCell};

/// Separates independently written hart state into 128-byte blocks.
///
/// This conservative performance alignment also separates 64-byte cache
/// lines; it is not a hardware cache-line-size or a correctness requirement.
#[repr(align(128))]
pub(crate) struct CacheAligned<T>(pub T);

impl<T> core::ops::Deref for CacheAligned<T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.0
    }
}

/// Hart-local state, consumed by the sbi extension layer.
pub struct HartLocal {
    /// Remote fence synchronization cell.
    pub rfence: RFenceCell,
    /// Type of inter-processor interrupt pending.
    pub ipi_type: CacheAligned<AtomicU8>,
    /// Supported hart features.
    features: Mutex<HartFeatures>,
}

impl HartLocal {
    fn new() -> Self {
        Self {
            rfence: RFenceCell::new(),
            ipi_type: CacheAligned(AtomicU8::new(0)),
            features: Mutex::new(HartFeatures::default()),
        }
    }

    /// Runs a read-only operation on this hart's detected features.
    pub(crate) fn with_features<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&HartFeatures) -> R,
    {
        f(&self.features.lock())
    }

    /// Runs an update on this hart's detected features.
    pub(crate) fn with_features_mut<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&mut HartFeatures) -> R,
    {
        f(&mut self.features.lock())
    }

}

static HART_LOCALS: Once<Box<[HartLocal]>> = Once::new();

/// Initializes all policy slots on the boot hart before platform discovery.
/// Secondary harts wait for platform publication before accessing them.
pub fn init() {
    HART_LOCALS.call_once(|| HartId::all().map(|_| HartLocal::new()).collect());
}

/// Forms the shared reference to `hart_id`'s state after initialization.
pub fn hart_local(hart_id: usize) -> &'static HartLocal {
    let hart = HartId::from_raw(hart_id).expect("BUG: unknown hart ID");
    slot(hart)
}

fn slot(hart: HartId) -> &'static HartLocal {
    &HART_LOCALS
        .get()
        .expect("BUG: hart-local state used before initialization")[hart.index()]
}

/// Runs `f` with shared access to the current hart's state. Mutable policy
/// fields are updated through their own locks, so this does not create an
/// aliasing contract for callers.
pub fn with_current<F, R>(f: F) -> R
where
    F: FnOnce(&HartLocal) -> R,
{
    let hart = HartId::current().expect("BUG: current hart is not in the boot topology");
    f(slot(hart))
}

/// Runs `f` with shared access to an arbitrary hart's state.
pub fn with_hart<F, R>(hart_id: usize, f: F) -> R
where
    F: FnOnce(&HartLocal) -> R,
{
    f(hart_local(hart_id))
}

/// Gets the local fence context for the current hart.
pub fn local_rfence() -> Option<LocalRFenceCell<'static>> {
    Some(slot(HartId::current().ok()?).rfence.local())
}

/// Gets the remote fence context for a specific hart.
pub fn remote_rfence(hart_id: usize) -> Option<RemoteRFenceCell<'static>> {
    let hart = HartId::from_raw(hart_id).ok()?;
    Some(slot(hart).rfence.remote())
}

