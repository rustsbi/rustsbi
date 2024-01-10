use sbi_spec::binary::{HartMask, SbiRet};

/// Inter-processor interrupt support.
pub trait Ipi {
    /// Send an inter-processor interrupt to all the harts defined in `hart_mask`.
    ///
    /// Inter-processor interrupts manifest at the receiving harts as the supervisor software interrupts.
    ///
    /// # Return value
    ///
    /// Should return `SbiRet::success()` if IPI was sent to all the targeted harts successfully.
    fn send_ipi(&self, hart_mask: HartMask) -> SbiRet;
}

impl<T: Ipi> Ipi for &T {
    #[inline]
    fn send_ipi(&self, hart_mask: HartMask) -> SbiRet {
        T::send_ipi(self, hart_mask)
    }
}
