use crate::ecall::SbiRet;

/// Hart State Management Extension
///
/// The Hart State Management (HSM) Extension introduces a set hart states and a set of functions 
/// which allow the supervisor-mode software to request a hart state change.
///
/// The possible hart states are as follows:
///
/// - STOPPED: The hart is not executing in supervisor-mode or any lower privilege mode. 
///   It is probably powered-down by the SBI implementation if the underlying platform has a mechanism 
///   to physically power-down harts.
/// - STARTED: The hart is physically powered-up and executing normally.
/// - START_PENDING: Some other hart has requested to start (or power-up) the hart from the STOPPED state
///   and the SBI implementation is still working to get the hart in the STARTED state.
/// - STOP_PENDING: The hart has requested to stop (or power-down) itself from the STARTED state
///   and the SBI implementation is still working to get the hart in the STOPPED state.
/// 
/// At any point in time, a hart should be in one of the above mentioned hart states.
///
/// Ref: [Section 8, RISC-V Supervisor Binary Interface Specification](https://github.com/riscv/riscv-sbi-doc/blob/master/riscv-sbi.adoc#8-hart-state-management-extension-eid-0x48534d-hsm)
pub trait Hsm: Send {
    /// Request the SBI implementation to start executing the given hart at specified address in supervisor-mode.
    ///
    /// This call is asynchronous — more specifically, the `sbi_hart_start()` may return before target hart 
    /// starts executing as long as the SBI implemenation is capable of ensuring the return code is accurate. 
    /// 
    /// It is recommended that if the SBI implementation is a platform runtime firmware executing in machine-mode (M-mode) 
    /// then it MUST configure PMP and other the M-mode state before executing in supervisor-mode.
    ///
    /// # Parameters
    /// 
    /// - The `hartid` parameter specifies the target hart which is to be started.
    /// - The `start_addr` parameter points to a runtime-specified physical address, where the hart can start executing in supervisor-mode.
    /// - The `opaque` parameter is a `usize` value which will be set in the `a1` register when the hart starts executing at `start_addr`.
    /// 
    /// # Return value
    ///
    /// The possible return error codes returned in `SbiRet.error` are shown in the table below:
    ///
    /// | Return code               | Description 
    /// |:--------------------------|:----------------------------------------------
    /// | SBI_SUCCESS               | Hart was previously in stopped state. It will start executing from `start_addr`.
    /// | SBI_ERR_INVALID_ADDRESS   | `start_addr` is not valid possibly due to following reasons: 1. It is not a valid physical address. 2. The address is prohibited by PMP to run in supervisor mode.
    /// | SBI_ERR_INVALID_PARAM     | `hartid` is not a valid hartid as corresponding hart cannot started in supervisor mode. 
    /// | SBI_ERR_ALREADY_AVAILABLE | The given hartid is already started.
    /// | SBI_ERR_FAILED            | The start request failed for unknown reasons.
    ///
    /// # Behavior
    ///
    /// The target hart jumps to supervisor-mode at address specified by `start_addr` with following values in specific registers.
    ///
    /// | Register Name | Register Value
    /// |:--------------|:--------------
    /// | `satp`        | 0
    /// | `sstatus.SIE` | 0
    /// | a0            | `hartid`
    /// | a1            | `opaque`
    fn hart_start(&mut self, hartid: usize, start_addr: usize, opaque: usize) -> SbiRet;
    /// Request the SBI implementation to stop executing the calling hart in supervisor-mode 
    /// and return it’s ownership to the SBI implementation. 
    ///
    /// This call is not expected to return under normal conditions. 
    /// The `sbi_hart_stop()` must be called with the supervisor-mode interrupts disabled.
    ///
    /// # Return value
    /// 
    /// The possible return error codes returned in `SbiRet.error` are shown in the table below:
    ///
    /// | Error code  | Description 
    /// |:------------|:------------
    /// | SBI_ERR_FAILED | Failed to stop execution of the current hart 
    fn hart_stop(&mut self, hartid: usize) -> SbiRet;
    /// Get the current status (or HSM state) of the given hart.
    ///
    /// The harts may transition HSM states at any time due to any concurrent `sbi_hart_start`
    /// or `sbi_hart_stop` calls, the return value from this function may not represent the actual state 
    /// of the hart at the time of return value verification.
    /// 
    /// # Parameters
    /// 
    /// The `hartid` parameter specifies the target hart which is to be started.
    ///
    /// # Return value
    ///
    /// The possible status values returned in `SbiRet.value` are shown in the table below:
    ///
    /// | Name          | Value | Description
    /// |:--------------|:------|:-------------------------
    /// | STARTED       |   0   | Hart Started
    /// | STOPPED       |   1   | Hart Stopped
    /// | START_PENDING |   2   | Hart start request pending
    /// | STOP_PENDING  |   3   | Hart stop request pending
    /// 
    /// The possible return error codes returned in `SbiRet.error` are shown in the table below:
    ///
    /// | Error code  | Description 
    /// |:------------|:------------
    /// | SBI_ERR_INVALID_PARAM | The given `hartid` or `start_addr` is not valid
    fn hart_get_status(&self, hartid: usize) -> SbiRet;
}

use alloc::boxed::Box;
use spin::Mutex;

lazy_static::lazy_static! {
    static ref HSM: Mutex<Option<Box<dyn Hsm>>> =
        Mutex::new(None);
}

#[doc(hidden)] // use through a macro or a call from implementation
pub fn init_hsm<T: Hsm + Send + 'static>(hsm: T) {
    *HSM.lock() = Some(Box::new(hsm));
}

#[inline]
pub(crate) fn probe_hsm() -> bool {
    HSM.lock().as_ref().is_some()
}

pub(crate) fn hart_start(hartid: usize, start_addr: usize, private_value: usize) -> SbiRet {
    if let Some(obj) = &mut *HSM.lock() {
        return obj.hart_start(hartid, start_addr, private_value);
    }
    SbiRet::not_supported()
}

pub(crate) fn hart_stop(hartid: usize) -> SbiRet {
    if let Some(obj) = &mut *HSM.lock() {
        return obj.hart_stop(hartid);
    }
    SbiRet::not_supported()
}

pub(crate) fn hart_get_status(hartid: usize) -> SbiRet {
    if let Some(obj) = &mut *HSM.lock() {
        return obj.hart_get_status(hartid);
    }
    SbiRet::not_supported()
}
