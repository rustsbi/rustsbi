use rustsbi::RustSBI;

pub mod console;
pub mod hsm;
pub mod ipi;
pub mod reset;
pub mod rfence;

pub mod extensions;
pub mod fifo;
pub mod hart_context;
pub mod logger;
pub mod trap;
pub mod trap_stack;

use console::{ConsoleDevice, SbiConsole};
use hsm::SbiHsm;
use ipi::{IpiDevice, SbiIpi};
use reset::{ResetDevice, SbiReset};
use rfence::SbiRFence;

#[derive(RustSBI, Default)]
#[rustsbi(dynamic)]
#[allow(clippy::upper_case_acronyms)]
pub struct SBI<C: ConsoleDevice, I: IpiDevice, R: ResetDevice> {
    #[rustsbi(console)]
    pub console: Option<SbiConsole<C>>,
    #[rustsbi(ipi, timer)]
    pub ipi: Option<SbiIpi<I>>,
    #[rustsbi(hsm)]
    pub hsm: Option<SbiHsm>,
    #[rustsbi(reset)]
    pub reset: Option<SbiReset<R>>,
    #[rustsbi(fence)]
    pub rfence: Option<SbiRFence>,
}

impl<C: ConsoleDevice, I: IpiDevice, R: ResetDevice> SBI<C, I, R> {
    pub const fn new() -> Self {
        SBI {
            console: None,
            ipi: None,
            hsm: None,
            reset: None,
            rfence: None,
        }
    }
}
