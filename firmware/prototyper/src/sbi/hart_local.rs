//! Per-hart SBI policy state.
//!
//! Slots are initialized once during boot. Mutable fields use atomics or locks.

use alloc::boxed::Box;
use core::sync::atomic::AtomicU8;
use runtime::hart::HartId;
use runtime::pmu::{self, CounterError, CounterTopology};
use spin::{Mutex, Once};

use super::features::HartFeatures;
use super::pmu::PmuState;
use super::rfence::queue::{LocalRFenceCell, RFenceCell, RemoteRFenceCell};

/// Hart-state storage aligned to 128 bytes to limit false sharing.
///
/// The alignment is a performance choice, not a detected cache-line size or
/// a hardware correctness requirement.
#[repr(align(128))]
pub(crate) struct CacheAligned<T>(pub T);

impl<T> core::ops::Deref for CacheAligned<T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.0
    }
}

/// Hart-local state consumed by the SBI extension layer.
pub struct HartLocal {
    /// Remote fence synchronization cell.
    pub rfence: RFenceCell,
    /// Pending firmware IPI types.
    pub ipi_type: CacheAligned<AtomicU8>,
    /// Supported hart features.
    features: Mutex<HartFeatures>,
    /// PMU state.
    pmu_state: Mutex<PmuState>,
}

impl HartLocal {
    fn new() -> Self {
        Self {
            rfence: RFenceCell::new(),
            ipi_type: CacheAligned(AtomicU8::new(0)),
            features: Mutex::new(HartFeatures::default()),
            pmu_state: Mutex::new(PmuState::new(0)),
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

    /// Runs an update or read on this hart's PMU state.
    pub(crate) fn with_pmu<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&mut PmuState) -> R,
    {
        f(&mut self.pmu_state.lock())
    }

    /// Rebuilds policy PMU state from Runtime's detected counter topology.
    pub(crate) fn init_pmu(&self, counters: CounterTopology) {
        self.with_pmu(|pmu| *pmu = PmuState::new(counters.mask()));
    }

    fn pmu_state_reset(&self) -> Result<(), CounterError> {
        let pmu = pmu::Pmu::current()?;
        pmu.reset()?;
        self.init_pmu(pmu.probe()?);
        Ok(())
    }
}

static HART_LOCALS: Once<Box<[HartLocal]>> = Once::new();

/// Initializes all policy slots on the boot hart after topology publication.
/// Secondary harts wait for platform publication before accessing them.
pub fn init() {
    assert!(HartId::count() > 0, "BUG: hart topology is not initialized");
    HART_LOCALS.call_once(|| HartId::all().map(|_| HartLocal::new()).collect());
}

/// Returns the shared state for an initialized, enabled `hart_id`.
pub fn hart_local(hart_id: usize) -> &'static HartLocal {
    let hart = HartId::from_raw(hart_id).expect("BUG: unknown hart ID");
    slot(hart)
}

fn slot(hart: HartId) -> &'static HartLocal {
    &HART_LOCALS
        .get()
        .expect("BUG: hart-local state used before initialization")[hart.index()]
}

/// Runs `f` with shared access to the current hart's state.
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

/// Returns the local fence context for the current hart.
pub fn local_rfence() -> Option<LocalRFenceCell<'static>> {
    Some(slot(HartId::current().ok()?).rfence.local())
}

/// Returns the remote fence context for a specific hart.
pub fn remote_rfence(hart_id: usize) -> Option<RemoteRFenceCell<'static>> {
    let hart = HartId::from_raw(hart_id).ok()?;
    Some(slot(hart).rfence.remote())
}

/// Resets the current hart's PMU state before a non-retentive resume.
/// Pending IPIs and fence requests remain valid across suspend/resume.
pub(crate) fn reset_current() -> Result<(), CounterError> {
    with_current(HartLocal::pmu_state_reset)
}
