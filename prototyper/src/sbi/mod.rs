use rustsbi::RustSBI;

pub mod console;
pub mod hsm;
pub mod ipi;
pub mod reset;
pub mod rfence;

pub mod fifo;
pub mod hart_context;
pub mod logger;
pub mod trap;
pub mod trap_stack;
pub mod extensions;

use console::{ConsoleDevice, SbiConsole};
use hsm::SbiHsm;
use ipi::{IpiDevice, SbiIpi};
use reset::{ResetDevice, SbiReset};
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
    pub rfence: Option<SbiRFence>,
}
