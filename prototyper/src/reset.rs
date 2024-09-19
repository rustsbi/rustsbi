use core::{
    ptr::null_mut,
    sync::atomic::{
        AtomicPtr,
        Ordering::{Relaxed, Release},
    },
};
use rustsbi::SbiRet;
use sifive_test_device::SifiveTestDevice;

pub(crate) static RESET: AtomicPtr<SifiveTestDevice> = AtomicPtr::new(null_mut());

pub fn init(base: usize) {
    RESET.store(base as _, Release);
}

pub fn fail() -> ! {
    let sifive_test = RESET.load(Relaxed);
    if sifive_test.is_null() {
        trace!("test fail, begin dead loop");
        loop {
            core::hint::spin_loop()
        }
    } else {
        trace!("test fail, invoke process exit procedure on SiFive Test device");
        unsafe { (*sifive_test).fail(0) }
    }
}


pub struct TestDevice<'a> {
    pub sifive_test: &'a AtomicPtr<SifiveTestDevice>,
}

impl<'a> TestDevice<'a> {
    pub fn new(sifive_test: &'a AtomicPtr<SifiveTestDevice>) -> Self {
        Self { sifive_test }
    }
}

impl<'a> rustsbi::Reset for TestDevice<'a> {
    #[inline]
    fn system_reset(&self, reset_type: u32, reset_reason: u32) -> SbiRet {
        use rustsbi::spec::srst::{
            RESET_REASON_NO_REASON, RESET_REASON_SYSTEM_FAILURE, RESET_TYPE_COLD_REBOOT,
            RESET_TYPE_SHUTDOWN, RESET_TYPE_WARM_REBOOT,
        };
        match reset_type {
            RESET_TYPE_SHUTDOWN => match reset_reason {
                RESET_REASON_NO_REASON => unsafe {
                    (*self.sifive_test.load(Relaxed)).pass();
                },
                RESET_REASON_SYSTEM_FAILURE => unsafe {
                    (*self.sifive_test.load(Relaxed)).fail(u16::MAX);
                },
                value => unsafe {
                    (*self.sifive_test.load(Relaxed)).fail(value as _);
                },
            },
            RESET_TYPE_COLD_REBOOT | RESET_TYPE_WARM_REBOOT => unsafe {
                (*self.sifive_test.load(Relaxed)).reset();
            },

            _ => SbiRet::invalid_param(),
        }
    }
}
