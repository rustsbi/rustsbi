use rustsbi::RustSBI;

pub mod console;
pub mod hsm;
pub mod ipi;
pub mod pmu;
pub mod reset;
pub mod rfence;

pub mod early_trap;
pub mod features;
pub mod fifo;
pub mod hart_context;
pub mod heap;
pub mod logger;
pub mod trap;
pub mod trap_stack;

use console::SbiConsole;
use hsm::SbiHsm;
use ipi::SbiIpi;
use pmu::SbiPmu;
use reset::SbiReset;
use rfence::SbiRFence;

#[derive(RustSBI, Default)]
#[rustsbi(dynamic)]
#[allow(clippy::upper_case_acronyms)]
pub struct SBI {
    #[rustsbi(console)]
    pub console: Option<SbiConsole>,
    #[rustsbi(ipi, timer)]
    pub ipi: Option<SbiIpi>,
    #[rustsbi(hsm)]
    pub hsm: Option<SbiHsm>,
    #[rustsbi(reset)]
    pub reset: Option<SbiReset>,
    #[rustsbi(fence)]
    pub rfence: Option<SbiRFence>,
    #[rustsbi(pmu)]
    pub pmu: Option<SbiPmu>,
}

impl SBI {
    pub const fn new() -> Self {
        SBI {
            console: None,
            ipi: None,
            hsm: None,
            reset: None,
            rfence: None,
            pmu: None,
        }
    }
}
