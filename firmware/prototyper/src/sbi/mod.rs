//! SBI extension implementations and the platform's published dispatcher.
//!
//! Runtime calls the dispatcher through the original RustSBI trait.

use alloc::boxed::Box;
use runtime::rustsbi::{self, RustSBI};
use runtime::{
    Result, console as runtime_console, events, ipi as runtime_ipi, memory::SupervisorMemory,
};
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

/// The SBI extensions provided by the firmware.
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

/// Publishes the SBI services after their Runtime devices are initialized.
pub(crate) fn init(
    supervisor_memory: SupervisorMemory,
    custom_extension: vendor::Extension,
    pmu: Option<SbiPmu>,
    hardware_wakeup: bool,
) -> Result<()> {
    static SUPERVISOR_MEMORY: Once<SupervisorMemory> = Once::new();
    let supervisor_memory = SUPERVISOR_MEMORY.call_once(|| supervisor_memory);
    let ipi = runtime_ipi::get().map(|_| SbiIpi);
    events::install(pmu.as_ref().map(|_| pmu::runtime_counters()));

    let console = runtime_console::get().map(|_| SbiConsole::new(supervisor_memory));
    let ipi_available = ipi.is_some();
    let hsm = ipi_available.then(|| SbiHsm::new(hardware_wakeup));
    let dispatcher: &'static SbiDispatcher = Box::leak(Box::new(SbiDispatcher {
        console,
        cppc: Some(SbiCppc::new()),
        dbtr: Some(SbiDbtr::new(supervisor_memory)),
        fwft: Some(SbiFwft),
        ipi,
        timer: SbiTimer,
        reset: SbiReset,
        rfence: ipi_available.then(SbiRFence::new).transpose()?,
        susp: hsm.as_ref().map(|_| SbiSuspend),
        hsm,
        pmu,
        sta: Some(SbiSta::new(supervisor_memory)),
        mpxy: Some(SbiMpxy::new(supervisor_memory)),
        vendor: vendor::Vendor::new(custom_extension, supervisor_memory),
    }));
    dispatcher.publish()
}

/// Returns the HSM extension, if present.
pub(crate) fn hsm() -> Option<&'static SbiHsm> {
    SbiDispatcher::get().and_then(|sbi| sbi.hsm.as_ref())
}

/// Returns the PMU extension, if present.
pub(crate) fn pmu() -> Option<&'static SbiPmu> {
    SbiDispatcher::get().and_then(|sbi| sbi.pmu.as_ref())
}
