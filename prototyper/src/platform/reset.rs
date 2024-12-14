use sifive_test_device::SifiveTestDevice;

use crate::sbi::reset::ResetDevice;
pub(crate) const SIFIVETEST_COMPATIBLE: [&str; 1] = ["sifive,test0"];

/// Reset Device: SifiveTestDevice
impl ResetDevice for SifiveTestDevice {
    #[inline]
    fn fail(&self, code: u16) -> ! {
        self.fail(code)
    }

    #[inline]
    fn pass(&self) -> ! {
        self.pass()
    }

    #[inline]
    fn reset(&self) -> ! {
        self.reset()
    }
}
