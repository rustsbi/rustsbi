use crate::ecall::SbiRet;
use crate::util::OnceFatBox;
use alloc::boxed::Box;

/// System Reset Extension
///
/// Provides a function that allow the supervisor software to request system-level reboot or shutdown.
///
/// The term "system" refers to the world-view of supervisor software and the underlying SBI implementation
/// could be machine mode firmware or hypervisor.
///
/// Ref: [Section 9, RISC-V Supervisor Binary Interface Specification](https://github.com/riscv-non-isa/riscv-sbi-doc/blob/master/riscv-sbi.adoc#9-system-reset-extension-eid-0x53525354-srst)
pub trait Reset: Send {
    /// Reset the system based on provided `reset_type` and `reset_reason`.
    ///
    /// This is a synchronous call and does not return if it succeeds.
    ///
    /// # Warm reboot and cold reboot
    ///
    /// When supervisor software is running natively, the SBI implementation is machine mode firmware.
    /// In this case, shutdown is equivalent to physical power down of the entire system and
    /// cold reboot is equivalent to physical power cycle of the entire system. Further, warm reboot
    /// is equivalent to a power cycle of main processor and parts of the system but not the entire system.
    ///
    /// For example, on a server class system with a BMC (board management controller),
    /// a warm reboot will not power cycle the BMC whereas a cold reboot will definitely power cycle the BMC.
    ///
    /// When supervisor software is running inside a virtual machine, the SBI implementation is a hypervisor.
    /// The shutdown, cold reboot and warm reboot will behave functionally the same as the native case but might
    /// not result in any physical power changes.
    ///
    /// # Return value
    ///
    /// The possible return error codes returned in `SbiRet.error` are shown in the table below:
    ///
    /// | Error code            | Description
    /// |:----------------------|:---------------
    /// | SBI_ERR_INVALID_PARAM | `reset_type` or `reset_reason` is not valid.
    /// | SBI_ERR_NOT_SUPPORTED | `reset_type` is valid but not implemented.
    /// | SBI_ERR_FAILED        | Reset request failed for unknown reasons.
    fn system_reset(&self, reset_type: usize, reset_reason: usize) -> SbiRet;

    /// Legacy extension's reset function
    ///
    /// Puts all the harts to shut down state from supervisor point of view. This SBI call doesn’t return.
    #[doc(hidden)]
    fn legacy_reset(&self) -> ! {
        // By default, this function redirects to `system_reset`.
        self.system_reset(RESET_TYPE_SHUTDOWN, RESET_REASON_NO_REASON);
        unreachable!()
    }
}

static RESET: OnceFatBox<dyn Reset + Sync + 'static> = OnceFatBox::new();

#[doc(hidden)]
#[allow(unused)]
pub const RESET_TYPE_SHUTDOWN: usize = 0x0000_0000;
#[doc(hidden)]
#[allow(unused)]
pub const RESET_TYPE_COLD_REBOOT: usize = 0x0000_0001;
#[doc(hidden)]
#[allow(unused)]
pub const RESET_TYPE_WARM_REBOOT: usize = 0x0000_0002;

#[doc(hidden)]
#[allow(unused)]
pub const RESET_REASON_NO_REASON: usize = 0x0000_0000;
#[doc(hidden)]
#[allow(unused)]
pub const RESET_REASON_SYSTEM_FAILURE: usize = 0x0000_0001;

#[doc(hidden)] // use through a macro
pub fn init_reset<T: Reset + Sync + 'static>(reset: T) {
    let result = RESET.set(Box::new(reset));
    if result.is_err() {
        panic!("load sbi module when already loaded")
    }
}

#[inline]
pub(crate) fn probe_reset() -> bool {
    RESET.get().is_some()
}

#[inline]
pub(crate) fn system_reset(reset_type: usize, reset_reason: usize) -> SbiRet {
    if let Some(obj) = RESET.get() {
        return obj.system_reset(reset_type, reset_reason);
    }
    SbiRet::not_supported()
}

#[inline]
pub(crate) fn legacy_reset() -> ! {
    if let Some(obj) = RESET.get() {
        obj.legacy_reset()
    }
    unreachable!("no reset handler available; this is okay if your platform didn't declare a legacy reset handler")
}
