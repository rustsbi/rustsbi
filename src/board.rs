//! Board support, including peripheral and core drivers.
use core::mem::MaybeUninit;
use rustsbi::{RustSBI, SbiRet};

use crate::clint::ClintDevice;
use crate::console::ConsoleDevice;
use crate::hsm::Hsm;

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
    pub sifive_test: Option<SifiveTestDevice<'a>>,
}

pub struct SifiveTestDevice<'a> {
    pub sifive_test: &'a sifive_test_device::SifiveTestDevice,
}

impl<'a> rustsbi::Reset for SifiveTestDevice<'a> {
    #[inline]
    fn system_reset(&self, reset_type: u32, reset_reason: u32) -> SbiRet {
        use rustsbi::spec::srst::{
            RESET_REASON_NO_REASON, RESET_REASON_SYSTEM_FAILURE, RESET_TYPE_COLD_REBOOT,
            RESET_TYPE_SHUTDOWN, RESET_TYPE_WARM_REBOOT,
        };
        match reset_type {
            RESET_TYPE_SHUTDOWN => match reset_reason {
                RESET_REASON_NO_REASON => self.sifive_test.pass(),
                RESET_REASON_SYSTEM_FAILURE => self.sifive_test.fail(-1 as _),
                value => self.sifive_test.fail(value as _),
            },
            RESET_TYPE_COLD_REBOOT | RESET_TYPE_WARM_REBOOT => {
                self.sifive_test.reset();
            }
            _ => SbiRet::invalid_param(),
        }
    }
}
