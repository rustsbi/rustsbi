use sbi_spec::binary::SbiRet;

/// Hart State Management extension.
///
/// The Hart State Management (HSM) Extension introduces a set hart states and a set of functions
/// which allow the supervisor-mode software to request a hart state change.
///
/// # Hart states
///
/// The possible hart states along with a unique State ID are as follows:
///
/// | State ID | State Name | Description
/// |:---------|:-----------|:------------
/// | 0 | `STARTED` | The hart is physically powered-up and executing normally.
/// | 1 | `STOPPED` | The hart is not executing in supervisor-mode or any lower privilege mode. It is probably powered-down by the SBI implementation if the underlying platform has a mechanism to physically power-down harts.
/// | 2 | `START_PENDING` | Some other hart has requested to start (or power-up) the hart from the **STOPPED** state and the SBI implementation is still working to get the hart in the **STARTED** state.
/// | 3 | `STOP_PENDING` | The hart has requested to stop (or power-down) itself from the STARTED state and the SBI implementation is still working to get the hart in the **STOPPED** state.
/// | 4 | `SUSPENDED` | This hart is in a platform specific suspend (or low power) state.
/// | 5 | `SUSPEND_PENDING` | The hart has requestd to put itself in a platform specific low power state from the **STARTED** state and the SBI implementation is still working to get the hart in the platform specific **SUSPENDED** state.
/// | 6 | `RESUME_PENDING` | An interrupt or platform specific hardware event has caused the hart to resume normal execution from the **SUSPENDED** state and the SBI implementation is still working to get the hart in the **STARTED** state.
///
/// At any point in time, a hart should be in one of the above mentioned hart states.
///
/// # Topology hart groups
///
/// A platform can have multiple harts grouped into a hierarchical topology groups (namely cores, clusters, nodes, etc.)
/// with separate platform specific low-power states for each hierarchical group.
/// These platform specific low-power states of hierarchial topology groups can be represented as platform specific suspend states of a hart.
/// A SBI implementation can utilize the suspend states of higher topology groups using one of the following approaches:
///
/// 1. *Platform-coordinated:* In this approach, when a hart becomes idle the supervisor-mode power-managment software
///   will request deepest suspend state for the hart and higher topology groups.
///   A SBI implementation should choose a suspend state at higher topology group which is:
/// - Not deeper than the specified suspend state
/// - Wake-up latency is not higher than the wake-up latency of the specified suspend state
/// 2. *OS-inititated:* In this approach, the supervisor-mode power-managment software will directly request a suspend state
///   for higher topology group after the last hart in that group becomes idle.
///   When a hart becomes idle, the supervisor-mode power-managment software will always select suspend state for the hart itself
///   but it will select a suspend state for a higher topology group only if the hart is the last running hart in the group.
///   A SBI implementation should:
/// - Never choose a suspend state for higher topology group different from the specified suspend state
/// - Always prefer most recent suspend state requested for higher topology group
///
/// Ref: [Section 8, RISC-V Supervisor Binary Interface Specification](https://github.com/riscv-non-isa/riscv-sbi-doc/blob/master/riscv-sbi.adoc#8-hart-state-management-extension-eid-0x48534d-hsm)
pub trait Hsm {
    /// Request the SBI implementation to start executing the given hart at specified address in supervisor-mode.
    ///
    /// This call is asynchronous - more specifically, the `hart_start()` may return before target hart
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
    /// *NOTE:* A single `usize` parameter is sufficient as `start_addr`,
    /// because the hart will start execution in supervisor-mode with the MMU off,
    /// hence `start_addr` must be less than XLEN bits wide.
    ///
    /// # Behavior
    ///
    /// The target hart jumps to supervisor mode at address specified by `start_addr` with specific register values described as follows.
    ///
    /// | Register Name | Register Value
    /// |:--------------|:--------------
    /// | `satp`        | 0
    /// | `sstatus.SIE` | 0
    /// | a0            | hartid
    /// | a1            | `opaque` parameter
    ///
    /// # Return value
    ///
    /// The possible return error codes returned in `SbiRet.error` are shown in the table below:
    ///
    /// | Return code                   | Description
    /// |:------------------------------|:----------------------------------------------
    /// | `SbiRet::success()`           | Hart was previously in stopped state. It will start executing from `start_addr`.
    /// | `SbiRet::invalid_address()`   | `start_addr` is not valid, possibly due to the following reasons: it is not a valid physical address, or executable access to the address is prohibited by a physical memory protection mechanism or H-extension G-stage for supervisor-mode.
    /// | `SbiRet::invalid_param()`     | `hartid` is not a valid hartid as corresponding hart cannot started in supervisor mode.
    /// | `SbiRet::already_available()` | The given hartid is already started.
    /// | `SbiRet::failed()`            | The start request failed for unspecified or unknown other reasons.
    fn hart_start(&self, hartid: usize, start_addr: usize, opaque: usize) -> SbiRet;
    /// Request the SBI implementation to stop executing the calling hart in supervisor-mode
    /// and return its ownership to the SBI implementation.
    ///
    /// This call is not expected to return under normal conditions.
    /// The `hart_stop()` must be called with supervisor-mode interrupts disabled.
    ///
    /// # Return value
    ///
    /// The possible return error codes returned in `SbiRet.error` are shown in the table below:
    ///
    /// | Error code         | Description
    /// |:-------------------|:------------
    /// | `SbiRet::failed()` | Failed to stop execution of the current hart
    fn hart_stop(&self) -> SbiRet;
    /// Get the current status (or HSM state id) of the given hart.
    ///
    /// The harts may transition HSM states at any time due to any concurrent `hart_start()`
    /// or `hart_stop()` calls, the return value from this function may not represent the actual state
    /// of the hart at the time of return value verification.
    ///
    /// # Parameters
    ///
    /// The `hartid` parameter specifies the target hart which status is required.
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
    /// | Error code                | Description
    /// |:--------------------------|:------------
    /// | `SbiRet::invalid_param()` | The given `hartid` is not valid
    fn hart_get_status(&self, hartid: usize) -> SbiRet;
    /// Request the SBI implementation to put the calling hart in a platform specfic suspend (or low power) state
    /// specified by the `suspend_type` parameter.
    ///
    /// The hart will automatically come out of suspended state and resume normal execution
    /// when it recieves an interrupt or platform specific hardware event.
    ///
    /// # Suspend behavior
    ///
    /// The platform specific suspend states for a hart can be either retentive or non-rententive in nature.
    ///
    /// A retentive suspend state will preserve hart register and CSR values for all privilege modes,
    /// whereas a non-retentive suspend state will not preserve hart register and CSR values.
    ///
    /// # Resuming
    ///
    /// Resuming from a retentive suspend state is straight forward and the supervisor-mode software
    /// will see SBI suspend call return without any failures.
    ///
    /// Resuming from a non-retentive suspend state is relatively more involved and requires software
    /// to restore various hart registers and CSRs for all privilege modes.
    /// Upon resuming from non-retentive suspend state, the hart will jump to supervisor-mode at address
    /// specified by `resume_addr` with specific registers values described in the table below:
    ///
    /// | Register Name | Register Value
    /// |:--------------|:--------------
    /// | `satp`        | 0
    /// | `sstatus.SIE` | 0
    /// | a0            | hartid
    /// | a1            | `opaque` parameter
    ///
    /// # Parameters
    ///
    /// The `suspend_type` parameter is 32 bits wide and the possible values are shown in the table below:
    ///
    /// | Value                   | Description
    /// |:------------------------|:--------------
    /// | 0x00000000              | Default retentive suspend
    /// | 0x00000001 - 0x0FFFFFFF | _Reserved for future use_
    /// | 0x10000000 - 0x7FFFFFFF | Platform specific retentive suspend
    /// | 0x80000000              | Default non-retentive suspend
    /// | 0x80000001 - 0x8FFFFFFF | _Reserved for future use_
    /// | 0x90000000 - 0xFFFFFFFF | Platform specific non-retentive suspend
    /// | > 0xFFFFFFFF            | _Reserved_
    ///
    /// The `resume_addr` parameter points to a runtime-specified physical address,
    /// where the hart can resume execution in supervisor-mode after a non-retentive
    /// suspend.
    ///
    /// *NOTE:* A single `usize` parameter is sufficient as `resume_addr`,
    /// because the hart will resume execution in supervisor-mode with the MMU off,
    /// hence `resume_addr` must be less than XLEN bits wide.
    ///
    /// The `opaque` parameter is an XLEN-bit value which will be set in the `a1`
    /// register when the hart resumes exectution at `resume_addr` after a
    /// non-retentive suspend.
    ///
    /// # Return value
    ///
    /// The possible return error codes returned in `SbiRet.error` are shown in the table below:
    ///
    /// | Error code                  | Description
    /// |:----------------------------|:------------
    /// | `SbiRet::success()`         | Hart has suspended and resumed back successfully from a retentive suspend state.
    /// | `SbiRet::invalid_param()`   | `suspend_type` is not valid.
    /// | `SbiRet::not_supported()`   | `suspend_type` is valid but not implemented.
    /// | `SbiRet::invalid_address()` | `resume_addr` is not valid, possibly due to the following reasons: it is not a valid physical address, or executable access to the address is prohibited by a physical memory protection mechanism or H-extension G-stage for supervisor-mode.
    /// | `SbiRet::failed()`          | The suspend request failed for unspecified or unknown other reasons.
    #[inline]
    fn hart_suspend(&self, suspend_type: u32, resume_addr: usize, opaque: usize) -> SbiRet {
        let _ = (suspend_type, resume_addr, opaque);
        SbiRet::not_supported()
    }
}

impl<T: Hsm> Hsm for &T {
    #[inline]
    fn hart_start(&self, hartid: usize, start_addr: usize, opaque: usize) -> SbiRet {
        T::hart_start(self, hartid, start_addr, opaque)
    }
    #[inline]
    fn hart_stop(&self) -> SbiRet {
        T::hart_stop(self)
    }
    #[inline]
    fn hart_get_status(&self, hartid: usize) -> SbiRet {
        T::hart_get_status(self, hartid)
    }
    #[inline]
    fn hart_suspend(&self, suspend_type: u32, resume_addr: usize, opaque: usize) -> SbiRet {
        T::hart_suspend(self, suspend_type, resume_addr, opaque)
    }
}
