//! Hart-local policy state: the per-hart bookkeeping consumed by the SBI
//! extension layer.
//!
//! This storage is policy state, deliberately decoupled from the
//! Runtime-owned trap stacks: the Runtime owns stack memory; the policy
//! firmware owns its own context records. Slots are initialized once during
//! boot, then their mutable fields are accessed through atomics or locks.

#![forbid(unsafe_code)]

use core::sync::atomic::AtomicU8;
use runtime::cfg::NUM_HART_MAX;
use runtime::hart::HartId;
use spin::{Mutex, Once};

use super::features::HartFeatures;
use super::pmu::PmuState;
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

    /// Rebuilds PMU state after feature detection has populated the counter mask.
    pub(crate) fn init_pmu(&self) {
        let mhpm_mask = self.with_features(HartFeatures::mhpm_mask);
        self.with_pmu(|pmu| *pmu = PmuState::new(mhpm_mask));
    }

    #[inline]
    fn pmu_state_reset(&self) {
        use super::features::PrivilegedVersion;
        // stop all hardware pmu event
        let hart_priv_version = self.with_features(HartFeatures::privileged_version);
        if hart_priv_version >= PrivilegedVersion::Version1_11 {
            // The CSR wrapper owns the machine-mode write; this policy state
            // only requests the reset of the current hart's counters.
            crate::riscv::csr::mcountinhibit::write_raw(!0b111usize);
        }
        // reset hart pmu state
        self.init_pmu();
    }
}

static HART_LOCALS: [Once<HartLocal>; NUM_HART_MAX] = [const { Once::new() }; NUM_HART_MAX];

/// Initializes all policy slots on the boot hart before platform discovery.
/// Secondary harts wait for platform publication before accessing them.
pub fn init() {
    for slot in &HART_LOCALS {
        slot.call_once(HartLocal::new);
    }
}

/// Forms the shared reference to `hart_id`'s state after initialization.
pub fn hart_local(hart_id: usize) -> &'static HartLocal {
    let slot = HART_LOCALS
        .get(hart_id)
        .expect("BUG: hart ID exceeds the configured limit");
    slot.get()
        .expect("BUG: hart-local state used before initialization")
}

/// Runs `f` with shared access to the current hart's state. Mutable policy
/// fields are updated through their own locks, so this does not create an
/// aliasing contract for callers.
pub fn with_current<F, R>(f: F) -> R
where
    F: FnOnce(&HartLocal) -> R,
{
    let hart_id = HartId::current()
        .expect("BUG: current hart exceeds Runtime capacity")
        .as_usize();
    f(hart_local(hart_id))
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
    let hart_id = HartId::current()
        .expect("BUG: current hart exceeds Runtime capacity")
        .as_usize();
    HART_LOCALS
        .get(hart_id)
        .map(|_| hart_local(hart_id).rfence.local())
}

/// Gets the remote fence context for a specific hart.
pub fn remote_rfence(hart_id: usize) -> Option<RemoteRFenceCell<'static>> {
    HART_LOCALS
        .get(hart_id)
        .map(|_| hart_local(hart_id).rfence.remote())
}

/// Resets the current hart's PMU state before a non-retentive resume.
/// Pending IPIs and fence requests remain valid across suspend/resume.
pub(crate) fn reset_current() {
    with_current(HartLocal::pmu_state_reset);
}
