//! Board support, including peripheral and core drivers.
use core::mem::MaybeUninit;
use rustsbi::RustSBI;

use crate::clint::ClintDevice;
use crate::reset::TestDevice;
use crate::console::ConsoleDevice;
use crate::hsm::Hsm;
use crate::rfence::RFence;

pub(crate) static mut SBI: MaybeUninit<Board> = MaybeUninit::uninit();

#[derive(RustSBI, Default)]
#[rustsbi(dynamic)]
pub struct Board<'a> {
    #[rustsbi(console)]
    pub uart16550: Option<ConsoleDevice<'a>>,
    #[rustsbi(ipi, timer)]
    pub clint: Option<ClintDevice<'a>>,
    #[rustsbi(hsm)]
    pub hsm: Option<Hsm>,
    #[rustsbi(reset)]
    pub sifive_test: Option<TestDevice<'a>>,
    #[rustsbi(fence)]
    pub rfence: Option<RFence>
}
