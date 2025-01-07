use sifive_test_device::SifiveTestDevice;

use crate::sbi::reset::ResetDevice;
pub(crate) const SIFIVETEST_COMPATIBLE: [&str; 1] = ["sifive,test0"];

pub struct SifiveTestDeviceWrap {
    inner: *const SifiveTestDevice,
}

impl SifiveTestDeviceWrap {
    pub fn new(base: usize) -> Self {
        Self {
            inner: base as *const SifiveTestDevice,
        }
    }
}

/// Reset Device: SifiveTestDevice
impl ResetDevice for SifiveTestDeviceWrap {
    #[inline]
    fn fail(&self, code: u16) -> ! {
        unsafe { (*self.inner).fail(code) }
    }

    #[inline]
    fn pass(&self) -> ! {
        unsafe { (*self.inner).pass() }
    }

    #[inline]
    fn reset(&self) -> ! {
        unsafe { (*self.inner).reset() }
    }
}
