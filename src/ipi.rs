use crate::ecall::SbiRet;
use crate::hart_mask::HartMask;
use crate::util::AmoOncePtr;

/// Inter-processor interrupt support
pub trait Ipi: Send + Sync {
    /// Send an inter-processor interrupt to all the harts defined in `hart_mask`.
    ///
    /// Inter-processor interrupts manifest at the receiving harts as the supervisor software interrupts.
    ///
    /// # Return value
    ///
    /// Should return error code `SBI_SUCCESS` if IPI was sent to all the targeted harts successfully.
    fn send_ipi_many(&self, hart_mask: HartMask) -> SbiRet;
    #[doc(hidden)]
    /// Get the maximum hart id available by this IPI support module
    fn max_hart_id(&self) -> usize {
        unimplemented!("remained for compatibility, should remove in 0.3.0")
    }
}

static IPI: AmoOncePtr<dyn Ipi> = AmoOncePtr::new();

pub fn init_ipi(ipi: &'static dyn Ipi) {
    if !IPI.try_call_once(ipi) {
        panic!("load sbi module when already loaded")
    }
}

#[inline]
pub(crate) fn probe_ipi() -> bool {
    IPI.get().is_some()
}

pub(crate) fn send_ipi_many(hart_mask: HartMask) -> SbiRet {
    if let Some(ipi) = IPI.get() {
        ipi.send_ipi_many(hart_mask)
    } else {
        SbiRet::not_supported()
    }
}
