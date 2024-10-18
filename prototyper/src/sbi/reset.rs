use core::sync::atomic::{AtomicPtr, Ordering::Relaxed};
use rustsbi::SbiRet;

use crate::board::SBI_IMPL;

pub trait ResetDevice {
    fn fail(&self, code: u16) -> !;
    fn pass(&self) -> !;
    fn reset(&self) -> !;
}

pub struct SbiReset<'a, T: ResetDevice> {
    pub reset_dev: &'a AtomicPtr<T>,
}

impl<'a, T: ResetDevice> SbiReset<'a, T> {
    pub fn new(reset_dev: &'a AtomicPtr<T>) -> Self {
        Self { reset_dev }
    }

    pub fn fail(&self) -> ! {
        let reset_dev = self.reset_dev.load(Relaxed);
        if reset_dev.is_null() {
            trace!("test fail, begin dead loop");
            loop {
                core::hint::spin_loop()
            }
        } else {
            trace!("Test fail, invoke process exit procedure on Reset device");
            unsafe { (*reset_dev).fail(0) }
        }
    }
}

impl<'a, T: ResetDevice> rustsbi::Reset for SbiReset<'a, T> {
    #[inline]
    fn system_reset(&self, reset_type: u32, reset_reason: u32) -> SbiRet {
        use rustsbi::spec::srst::{
            RESET_REASON_NO_REASON, RESET_REASON_SYSTEM_FAILURE, RESET_TYPE_COLD_REBOOT,
            RESET_TYPE_SHUTDOWN, RESET_TYPE_WARM_REBOOT,
        };
        match reset_type {
            RESET_TYPE_SHUTDOWN => match reset_reason {
                RESET_REASON_NO_REASON => unsafe {
                    (*self.reset_dev.load(Relaxed)).pass();
                },
                RESET_REASON_SYSTEM_FAILURE => unsafe {
                    (*self.reset_dev.load(Relaxed)).fail(u16::MAX);
                },
                value => unsafe {
                    (*self.reset_dev.load(Relaxed)).fail(value as _);
                },
            },
            RESET_TYPE_COLD_REBOOT | RESET_TYPE_WARM_REBOOT => unsafe {
                (*self.reset_dev.load(Relaxed)).reset();
            },

            _ => SbiRet::invalid_param(),
        }
    }
}

pub fn fail() -> ! {
    unsafe { SBI_IMPL.assume_init_ref() }
        .reset
        .as_ref()
        .unwrap()
        .fail()
}
