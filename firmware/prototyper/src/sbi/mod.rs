//! SBI extension implementations and the platform's published dispatcher.
//!
//! Platform boot selects the extension fields. Runtime calls the dispatcher
//! through the original RustSBI trait; device ownership stays in the drivers.

use spin::Once;

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
pub mod hart_local;
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

#[derive(runtime::rustsbi::RustSBI, Default)]
#[rustsbi(dynamic, crate = runtime::rustsbi)]
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
    pub(crate) timer: Option<SbiTimer>,
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

/// The SBI extension set, owned by the sbi layer.
///
/// Published once after all extension constructors run and before platform
/// initialization releases the secondary harts.
pub(crate) static SBI_DISPATCHER: Once<SbiDispatcher> = Once::new();

/// Returns the ipi extension, if present.
pub(crate) fn ipi() -> Option<&'static SbiIpi> {
    SBI_DISPATCHER.get().and_then(|sbi| sbi.ipi.as_ref())
}

/// Returns the hsm extension, if present.
pub(crate) fn hsm() -> Option<&'static SbiHsm> {
    SBI_DISPATCHER.get().and_then(|sbi| sbi.hsm.as_ref())
}

/// Returns the rfence extension, if present.
pub(crate) fn rfence() -> Option<&'static SbiRFence> {
    SBI_DISPATCHER.get().and_then(|sbi| sbi.rfence.as_ref())
}

/// Returns the pmu extension, if present.
pub(crate) fn pmu() -> Option<&'static SbiPmu> {
    SBI_DISPATCHER.get().and_then(|sbi| sbi.pmu.as_ref())
}

/// Returns the susp extension, if present.
pub(crate) fn susp() -> Option<&'static SbiSuspend> {
    SBI_DISPATCHER.get().and_then(|sbi| sbi.susp.as_ref())
}
