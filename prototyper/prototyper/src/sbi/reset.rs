use alloc::boxed::Box;
use rustsbi::SbiRet;
use spin::Mutex;

use crate::platform::PLATFORM;

pub trait ResetDevice {
    fn fail(&self, code: u16) -> !;
    fn pass(&self) -> !;
    fn reset(&self) -> !;
}

pub struct SbiReset {
    pub reset_dev: Mutex<Box<dyn ResetDevice>>,
}

impl SbiReset {
    pub fn new(reset_dev: Mutex<Box<dyn ResetDevice>>) -> Self {
        Self { reset_dev }
    }

    #[allow(unused)]
    pub fn fail(&self) -> ! {
        trace!("Test fail, invoke process exit procedure on Reset device");
        self.reset_dev.lock().fail(0);
    }
}

impl rustsbi::Reset for SbiReset {
    #[inline]
    fn system_reset(&self, reset_type: u32, reset_reason: u32) -> SbiRet {
        use rustsbi::spec::srst::{
            RESET_REASON_NO_REASON, RESET_REASON_SYSTEM_FAILURE, RESET_TYPE_COLD_REBOOT,
            RESET_TYPE_SHUTDOWN, RESET_TYPE_WARM_REBOOT,
        };
        match reset_type {
            RESET_TYPE_SHUTDOWN => match reset_reason {
                RESET_REASON_NO_REASON => self.reset_dev.lock().pass(),
                RESET_REASON_SYSTEM_FAILURE => self.reset_dev.lock().fail(u16::MAX),
                value => self.reset_dev.lock().fail(value as _),
            },
            RESET_TYPE_COLD_REBOOT | RESET_TYPE_WARM_REBOOT => self.reset_dev.lock().reset(),

            _ => SbiRet::invalid_param(),
        }
    }
}

#[allow(unused)]
pub fn fail() -> ! {
    match unsafe { PLATFORM.sbi.reset.as_ref() } {
        Some(reset) => reset.fail(),
        None => {
            trace!("test fail, begin dead loop");
            loop {
                core::hint::spin_loop()
            }
        }
    }
}
