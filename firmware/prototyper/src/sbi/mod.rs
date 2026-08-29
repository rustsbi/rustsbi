use rustsbi::{RustSBI, SbiRet};
use spin::Once;

pub mod console;
pub mod cppc;
pub mod hsm;
pub mod ipi;
pub mod mpxy;
pub mod pmu;
pub mod reset;
pub mod rfence;
pub mod suspend;

pub mod early_trap;
pub mod features;
pub mod hart_context;
pub mod heap;
pub mod logger;
pub mod trap;
pub mod trap_stack;

use console::SbiConsole;
use cppc::SbiCppc;
use hsm::SbiHsm;
use ipi::SbiIpi;
use mpxy::SbiMpxy;
use pmu::SbiPmu;
use reset::SbiReset;
use rfence::SbiRFence;
use suspend::SbiSuspend;

#[derive(RustSBI, Default)]
#[rustsbi(dynamic)]
pub struct SbiDispatcher {
    #[rustsbi(console)]
    console: Option<SbiConsole>,
    #[rustsbi(cppc)]
    cppc: Option<SbiCppc>,
    #[rustsbi(ipi, timer)]
    ipi: Option<SbiIpi>,
    #[rustsbi(hsm)]
    hsm: Option<SbiHsm>,
    #[rustsbi(reset)]
    reset: Option<SbiReset>,
    #[rustsbi(fence)]
    rfence: Option<SbiRFence>,
    #[rustsbi(pmu)]
    pmu: Option<SbiPmu>,
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
        ipi: Option<SbiIpi>,
        hsm: Option<SbiHsm>,
        reset: Option<SbiReset>,
        rfence: Option<SbiRFence>,
        susp: Option<SbiSuspend>,
        pmu: Option<SbiPmu>,
        mpxy: Option<SbiMpxy>,
    ) -> Self {
        SbiDispatcher {
            console,
            cppc,
            ipi,
            hsm,
            reset,
            rfence,
            pmu,
            susp,
            mpxy,
        }
    }
}

/// The SBI extension set, owned by the sbi layer.
///
/// Invariant: published once by the boot composition (`init_board`) after
/// all extension constructors have run and before `IS_K1_PLATFORM` is
/// stored and `platform::READY` is released; read afterwards through the
/// shared accessors below. Publishing and reading go through `spin::Once`,
/// so this half of the split is fully safe.
pub(crate) static SBI_DISPATCHER: Once<SbiDispatcher> = Once::new();

/// Dispatches an SBI ecall to the matching extension; the sole
/// whole-instance user of the dispatcher.
///
/// Pre-publish ecalls are unreachable: the trap vector only installs in
/// main's phase 4, after the dispatcher has been published.
pub(crate) fn handle_ecall(extension: usize, function: usize, param: [usize; 6]) -> SbiRet {
    SBI_DISPATCHER
        .get()
        .expect("dispatcher published before ecall handling; mtvec installs in main phase 4, after publish")
        .handle_ecall(extension, function, param)
}

/// Returns the ipi extension, if present.
pub(crate) fn ipi() -> Option<&'static SbiIpi> {
    SBI_DISPATCHER.get().and_then(|sbi| sbi.ipi.as_ref())
}

/// Returns the cppc extension, if present.
pub(crate) fn cppc() -> Option<&'static SbiCppc> {
    SBI_DISPATCHER.get().and_then(|sbi| sbi.cppc.as_ref())
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
