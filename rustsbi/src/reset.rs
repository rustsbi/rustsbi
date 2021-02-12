/// System Reset Extension
///
/// Provides a function that allow the supervisor software to request system-level reboot or shutdown.
/// 
/// The term "system" refers to the world-view of supervisor software and the underlying SBI implementation 
/// could be machine mode firmware or hypervisor.
///
/// Ref: [Section 9, RISC-V Supervisor Binary Interface Specification](https://github.com/riscv/riscv-sbi-doc/blob/master/riscv-sbi.adoc#9-system-reset-extension-eid-0x53525354-srst)
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
    /// Returns SBI_ERR_INVALID_PARAM, SBI_ERR_NOT_SUPPORTED or SBI_ERR_FAILED through `SbiRet.error` upon failure.
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

use alloc::boxed::Box;
use spin::Mutex;

use crate::ecall::SbiRet;

lazy_static::lazy_static! {
    static ref RESET: Mutex<Option<Box<dyn Reset>>> =
        Mutex::new(None);
}

#[doc(hidden)] #[allow(unused)]
pub const RESET_TYPE_SHUTDOWN: usize = 0x0000_0000;
#[doc(hidden)] #[allow(unused)]
pub const RESET_TYPE_COLD_REBOOT: usize = 0x0000_0001;
#[doc(hidden)] #[allow(unused)]
pub const RESET_TYPE_WARM_REBOOT: usize = 0x0000_0002;

#[doc(hidden)] #[allow(unused)]
pub const RESET_REASON_NO_REASON: usize = 0x0000_0000;
#[doc(hidden)] #[allow(unused)]
pub const RESET_REASON_SYSTEM_FAILURE: usize = 0x0000_0001;

#[doc(hidden)] // use through a macro
pub fn init_reset<T: Reset + Send + 'static>(reset: T) {
    *RESET.lock() = Some(Box::new(reset));
}

#[inline]
pub(crate) fn probe_reset() -> bool {
    RESET.lock().as_ref().is_some()
}

pub(crate) fn system_reset(reset_type: usize, reset_reason: usize) -> SbiRet {
    if let Some(obj) = &*RESET.lock() {
        return obj.system_reset(reset_type, reset_reason);
    }
    SbiRet::not_supported()
}

pub(crate) fn legacy_reset() -> ! {
    if let Some(obj) = &*RESET.lock() {
        obj.legacy_reset()
    }
    unreachable!("no reset handler available; this is okay if your platform didn't declare a legacy reset handler")
}
