//! SBI extension implementations and the platform's published dispatcher.
//!
//! Platform boot selects the extension fields. Runtime calls the dispatcher
//! through the original RustSBI trait and borrows its timer and IPI objects.

use runtime::rustsbi::{self, RustSBI};

pub mod console;
pub mod cppc;
pub mod dbtr;
pub mod fwft;
pub(crate) mod hsm;
pub mod ipi;
pub mod mpxy;
pub mod pmu;
pub mod reset;
pub mod rfence;
pub mod sta;
pub mod suspend;
pub mod timer;
pub(crate) mod vendor;

pub mod features;
pub mod logger;

use console::SbiConsole;
use cppc::SbiCppc;
use dbtr::SbiDbtr;
use fwft::SbiFwft;
use hsm::SbiHsm;
use ipi::SbiIpi;
use mpxy::SbiMpxy;
use pmu::SbiPmu;
use reset::SbiReset;
use rfence::SbiRFence;
use sta::SbiSta;
use suspend::SbiSuspend;
use timer::SbiTimer;

#[runtime::sbi_policy]
#[derive(RustSBI)]
#[rustsbi(dynamic, crate = rustsbi)]
pub struct SbiDispatcher {
    #[rustsbi(console)]
    pub(crate) console: Option<SbiConsole>,
    #[rustsbi(cppc)]
    pub(crate) cppc: Option<SbiCppc>,
    #[rustsbi(dbtr)]
    pub(crate) dbtr: Option<SbiDbtr>,
    #[rustsbi(fwft)]
    pub(crate) fwft: Option<SbiFwft>,
    #[rustsbi(ipi)]
    pub(crate) ipi: Option<SbiIpi>,
    #[rustsbi(timer)]
    pub(crate) timer: SbiTimer,
    #[rustsbi(hsm)]
    pub(crate) hsm: Option<SbiHsm>,
    #[rustsbi(reset)]
    pub(crate) reset: SbiReset,
    #[rustsbi(fence)]
    pub(crate) rfence: Option<SbiRFence>,
    #[rustsbi(pmu)]
    pub(crate) pmu: Option<SbiPmu>,
    #[rustsbi(sta)]
    pub(crate) sta: Option<SbiSta>,
    #[rustsbi(susp)]
    pub(crate) susp: Option<SbiSuspend>,
    #[rustsbi(mpxy)]
    pub(crate) mpxy: Option<SbiMpxy>,
    #[rustsbi(vendor)]
    pub(crate) vendor: Option<vendor::Vendor>,
}

/// Returns the published SBI timer policy.
pub(crate) fn timer() -> &'static SbiTimer {
    &SbiDispatcher::get()
        .expect("SBI dispatcher not initialized")
        .timer
}

/// Returns the HSM extension, if present.
pub(crate) fn hsm() -> Option<&'static SbiHsm> {
    SbiDispatcher::get().and_then(|sbi| sbi.hsm.as_ref())
}

/// Returns the RFence extension, if present.
pub(crate) fn rfence() -> Option<&'static SbiRFence> {
    SbiDispatcher::get().and_then(|sbi| sbi.rfence.as_ref())
}

/// Returns the SUSP extension, if present.
pub(crate) fn susp() -> Option<&'static SbiSuspend> {
    SbiDispatcher::get().and_then(|sbi| sbi.susp.as_ref())
}

/// Returns the PMU extension, if present.
pub(crate) fn pmu() -> Option<&'static SbiPmu> {
    SbiDispatcher::get().and_then(|sbi| sbi.pmu.as_ref())
}
