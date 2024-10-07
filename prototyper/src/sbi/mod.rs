use rustsbi::RustSBI;

pub mod console;
pub mod rfence;
pub mod ipi;
pub mod hsm;
pub mod reset;

pub mod fifo;
pub mod logger;
pub mod hart_context;
pub mod trap;
pub mod trap_stack;

use console::{SbiConsole, ConsoleDevice};
use ipi::{SbiIpi, IpiDevice};
use reset::{SbiReset, ResetDevice};
use hsm::SbiHsm;
use rfence::SbiRFence;


#[derive(RustSBI, Default)]
#[rustsbi(dynamic)]
pub struct SBI<'a, C: ConsoleDevice, I: IpiDevice, R: ResetDevice> {
    #[rustsbi(console)]
    pub console: Option<SbiConsole<'a, C>>,
    #[rustsbi(ipi, timer)]
    pub ipi: Option<SbiIpi<'a, I>>,
    #[rustsbi(hsm)]
    pub hsm: Option<SbiHsm>,
    #[rustsbi(reset)]
    pub reset: Option<SbiReset<'a, R>>,
    #[rustsbi(fence)]
    pub rfence: Option<SbiRFence>
}
