use rustsbi::{RustSBI, SbiRet};
use spin::Once;

pub mod console;
pub mod cppc;
pub mod dbtr;
pub mod fwft;
pub mod hsm;
pub mod ipi;
pub mod mpxy;
pub mod nacl;
pub mod pmu;
pub mod reset;
pub mod rfence;
pub mod sse;
pub mod sta;
pub mod suspend;
pub mod timer;

pub mod early_trap;
pub mod features;
pub mod hart_context;
pub mod heap;
pub mod logger;
pub mod trap;
pub mod trap_stack;

use console::SbiConsole;
use cppc::SbiCppc;
use dbtr::SbiDbtr;
use fwft::SbiFwft;
use hsm::SbiHsm;
use ipi::SbiIpi;
use mpxy::SbiMpxy;
use nacl::SbiNacl;
use pmu::SbiPmu;
use reset::SbiReset;
use rfence::SbiRFence;
use sse::SbiSse;
use sta::SbiSta;
use suspend::SbiSuspend;
use timer::SbiTimer;

#[derive(RustSBI, Default)]
#[rustsbi(dynamic)]
pub struct SbiDispatcher {
    #[rustsbi(console)]
    console: Option<SbiConsole>,
    #[rustsbi(cppc)]
    cppc: Option<SbiCppc>,
    #[rustsbi(dbtr)]
    dbtr: Option<SbiDbtr>,
    #[rustsbi(fwft)]
    fwft: Option<SbiFwft>,
    #[rustsbi(ipi)]
    ipi: Option<SbiIpi>,
    #[rustsbi(timer)]
    timer: Option<SbiTimer>,
    #[rustsbi(hsm)]
    hsm: Option<SbiHsm>,
    #[rustsbi(reset)]
    reset: Option<SbiReset>,
    #[rustsbi(fence)]
    rfence: Option<SbiRFence>,
    #[rustsbi(pmu)]
    pmu: Option<SbiPmu>,
    #[rustsbi(sta)]
    sta: Option<SbiSta>,
    #[rustsbi(nacl)]
    nacl: Option<SbiNacl>,
    #[rustsbi(sse)]
    sse: Option<SbiSse>,
    #[rustsbi(susp)]
    susp: Option<SbiSuspend>,
    #[rustsbi(mpxy)]
    mpxy: Option<SbiMpxy>,
}

impl SbiDispatcher {
    /// Assembles the dispatcher from the constructed extensions; the boot
    /// composition publishes the result via [`SBI_DISPATCHER`].
    pub(crate) fn new(
        console: Option<SbiConsole>,
        cppc: Option<SbiCppc>,
        dbtr: Option<SbiDbtr>,
        fwft: Option<SbiFwft>,
        ipi: Option<SbiIpi>,
        timer: Option<SbiTimer>,
        hsm: Option<SbiHsm>,
        reset: Option<SbiReset>,
        rfence: Option<SbiRFence>,
        susp: Option<SbiSuspend>,
        pmu: Option<SbiPmu>,
        sta: Option<SbiSta>,
        mpxy: Option<SbiMpxy>,
        nacl: Option<SbiNacl>,
        sse: Option<SbiSse>,
    ) -> Self {
        SbiDispatcher {
            console,
            cppc,
            dbtr,
            fwft,
            ipi,
            timer,
            hsm,
            reset,
            rfence,
            pmu,
            sta,
            sse,
            susp,
            mpxy,
            nacl,
        }
    }
}

/// The SBI extension set, owned by the sbi layer.
///
/// Published once after all extension constructors run and before platform
/// initialization releases the secondary harts.
pub(crate) static SBI_DISPATCHER: Once<SbiDispatcher> = Once::new();

/// Dispatches an SBI ecall to the matching extension; the sole
/// whole-instance user of the dispatcher.
///
/// Pre-publish ecalls are unreachable: the trap vector only installs in
/// main's phase 4, after the dispatcher has been published.
pub(crate) fn handle_ecall(extension: usize, function: usize, param: [usize; 6]) -> SbiRet {
    SBI_DISPATCHER
        .get()
        .expect("BUG: SBI ecall handled before dispatcher publication")
        .handle_ecall(extension, function, param)
}

/// Returns the console extension, if present.
pub(crate) fn console() -> Option<&'static SbiConsole> {
    SBI_DISPATCHER.get().and_then(|sbi| sbi.console.as_ref())
}

/// Returns the ipi extension, if present.
pub(crate) fn ipi() -> Option<&'static SbiIpi> {
    SBI_DISPATCHER.get().and_then(|sbi| sbi.ipi.as_ref())
}

/// Returns the timer extension, if present.
pub(crate) fn timer() -> Option<&'static SbiTimer> {
    SBI_DISPATCHER.get().and_then(|sbi| sbi.timer.as_ref())
}

/// Returns the hsm extension, if present.
pub(crate) fn hsm() -> Option<&'static SbiHsm> {
    SBI_DISPATCHER.get().and_then(|sbi| sbi.hsm.as_ref())
}

/// Returns the reset extension, if present.
pub(crate) fn reset() -> Option<&'static SbiReset> {
    SBI_DISPATCHER.get().and_then(|sbi| sbi.reset.as_ref())
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
