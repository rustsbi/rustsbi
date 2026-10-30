//! SBI extension implementations and the platform's published dispatcher.
//!
//! Platform boot selects the extension fields. Runtime calls the dispatcher
//! through the original RustSBI trait and borrows its owned Runtime objects.

use runtime::{
    rustsbi::{self, RustSBI},
    timer::Timer,
    trap::{Exception, TrapPolicy},
};
use spin::Once;

pub(crate) mod console;
pub(crate) mod cppc;
pub(crate) mod dbtr;
pub(crate) mod fwft;
pub(crate) mod hsm;
pub(crate) mod ipi;
pub(crate) mod mpxy;
pub(crate) mod pmu;
pub(crate) mod reset;
pub(crate) mod rfence;
pub(crate) mod sta;
pub(crate) mod suspend;
pub(crate) mod timer;
pub(crate) mod vendor;

pub(crate) mod logger;

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

#[derive(RustSBI)]
#[rustsbi(dynamic, crate = rustsbi)]
pub(crate) struct SbiDispatcher {
    #[rustsbi(console)]
    pub(crate) console: Option<SbiConsole>,
    #[rustsbi(cppc)]
    pub(crate) cppc: Option<SbiCppc>,
    #[rustsbi(dbtr)]
    pub(crate) dbtr: Option<SbiDbtr>,
    #[rustsbi(fwft)]
    pub(crate) fwft: Option<SbiFwft>,
    #[rustsbi(ipi)]
    pub(crate) ipi: SbiIpi,
    #[rustsbi(timer)]
    pub(crate) timer: SbiTimer,
    #[rustsbi(hsm)]
    pub(crate) hsm: Option<SbiHsm>,
    #[rustsbi(reset)]
    pub(crate) reset: SbiReset,
    #[rustsbi(fence)]
    pub(crate) rfence: Option<SbiRFence>,
    #[rustsbi(pmu)]
    pub(crate) pmu: SbiPmu,
    #[rustsbi(sta)]
    pub(crate) sta: Option<SbiSta>,
    #[rustsbi(susp)]
    pub(crate) susp: Option<SbiSuspend>,
    #[rustsbi(mpxy)]
    pub(crate) mpxy: Option<SbiMpxy>,
    #[rustsbi(vendor)]
    pub(crate) vendor: Option<vendor::Vendor>,
}

impl TrapPolicy for SbiDispatcher {
    fn on_nonretentive_resume(&self) {
        self.pmu
            .reset_current_hart_records()
            .expect("could not reset PMU policy records");
    }

    fn on_exception(&self, exception: Exception) {
        if let Some(event) = pmu::from_exception(exception) {
            SbiPmu::record_event(event);
        }
    }
}

/// The SBI extension set, owned by the sbi layer.
///
/// Published once after all extension constructors run and before platform
/// initialization releases the secondary harts.
pub(crate) static SBI_DISPATCHER: Once<SbiDispatcher> = Once::new();

/// Returns the ipi extension, if present.
pub(crate) fn ipi() -> Option<&'static SbiIpi> {
    SBI_DISPATCHER
        .get()
        .map(|sbi| &sbi.ipi)
        .filter(|sbi| sbi.ipi().delivery().is_some())
}

/// Returns the published timer for firmware counter reads.
pub(crate) fn timer() -> &'static Timer {
    SBI_DISPATCHER
        .get()
        .expect("SBI dispatcher not initialized")
        .timer
        .timer()
}

/// Returns the hsm extension, if present.
pub(crate) fn hsm() -> Option<&'static SbiHsm> {
    SBI_DISPATCHER.get().and_then(|sbi| sbi.hsm.as_ref())
}

/// Returns the rfence extension, if present.
pub(crate) fn rfence() -> Option<&'static SbiRFence> {
    SBI_DISPATCHER.get().and_then(|sbi| sbi.rfence.as_ref())
}

/// Returns the published pmu extension.
pub(crate) fn pmu() -> &'static SbiPmu {
    SBI_DISPATCHER
        .get()
        .map(|sbi| &sbi.pmu)
        .expect("SBI PMU is published before event accounting")
}

/// Returns the susp extension, if present.
pub(crate) fn susp() -> Option<&'static SbiSuspend> {
    SBI_DISPATCHER.get().and_then(|sbi| sbi.susp.as_ref())
}
