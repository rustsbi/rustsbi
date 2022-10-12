use sbi_spec::binary::SbiRet;

/// System Reset Extension
///
/// Provides a function that allow the supervisor software to request system-level reboot or shutdown.
///
/// The term "system" refers to the world-view of supervisor software and the underlying SBI implementation
/// could be machine mode firmware or hypervisor.
///
/// Ref: [Section 9, RISC-V Supervisor Binary Interface Specification](https://github.com/riscv-non-isa/riscv-sbi-doc/blob/master/riscv-sbi.adoc#9-system-reset-extension-eid-0x53525354-srst)
pub trait Reset: Send + Sync {
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
    /// | Error code                | Description
    /// |:--------------------------|:---------------
    /// | `SbiRet::invalid_param()` | `reset_type` or `reset_reason` is not valid.
    /// | `SbiRet::not_supported()` | `reset_type` is valid but not implemented.
    /// | `SbiRet::failed()`        | Reset request failed for unknown reasons.
    fn system_reset(&self, reset_type: u32, reset_reason: u32) -> SbiRet;

    /// Legacy extension's reset function
    ///
    /// Puts all the harts to shut down state from supervisor point of view. This SBI call doesn’t return.
    #[cfg(feature = "legacy")]
    fn legacy_reset(&self) -> ! {
        use sbi_spec::srst::{RESET_REASON_NO_REASON, RESET_TYPE_SHUTDOWN};
        // By default, this function redirects to `system_reset`.
        self.system_reset(RESET_TYPE_SHUTDOWN, RESET_REASON_NO_REASON);
        unreachable!()
    }
}

use crate::util::AmoOnceRef;

static RESET: AmoOnceRef<dyn Reset> = AmoOnceRef::new();

/// Init SRST module
pub fn init_reset(reset: &'static dyn Reset) {
    if !RESET.try_call_once(reset) {
        panic!("load sbi module when already loaded")
    }
}

#[inline]
pub(crate) fn probe_reset() -> bool {
    RESET.get().is_some()
}

#[inline]
pub(crate) fn system_reset(reset_type: u32, reset_reason: u32) -> SbiRet {
    if let Some(obj) = RESET.get() {
        return obj.system_reset(reset_type, reset_reason);
    }
    SbiRet::not_supported()
}

#[cfg(feature = "legacy")]
#[inline]
pub(crate) fn legacy_reset() -> ! {
    if let Some(obj) = RESET.get() {
        obj.legacy_reset()
    }
    unreachable!("no reset handler available; this is okay if your platform didn't declare a legacy reset handler")
}
