use crate::{hart_mask::HartMask, util::AmoOnceRef};
use sbi_spec::binary::SbiRet;

/// Inter-processor interrupt support
pub trait Ipi: Send + Sync {
    /// Send an inter-processor interrupt to all the harts defined in `hart_mask`.
    ///
    /// Inter-processor interrupts manifest at the receiving harts as the supervisor software interrupts.
    ///
    /// # Return value
    ///
    /// Should return `SbiRet::success()` if IPI was sent to all the targeted harts successfully.
    fn send_ipi(&self, hart_mask: HartMask) -> SbiRet;
}

static IPI: AmoOnceRef<dyn Ipi> = AmoOnceRef::new();

/// Init IPI module
pub fn init_ipi(ipi: &'static dyn Ipi) {
    if !IPI.try_call_once(ipi) {
        panic!("load sbi module when already loaded")
    }
}

#[inline]
pub(crate) fn probe_ipi() -> bool {
    IPI.get().is_some()
}

#[inline]
pub(crate) fn send_ipi(hart_mask: HartMask) -> SbiRet {
    if let Some(ipi) = IPI.get() {
        return ipi.send_ipi(hart_mask);
    }
    SbiRet::not_supported()
}
