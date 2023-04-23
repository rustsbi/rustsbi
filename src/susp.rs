use sbi_spec::binary::SbiRet;

/// System Suspend Extension
///
/// The system suspend extension defines a set of system-level sleep states and a
/// function which allows the supervisor-mode software to request that the system
/// transitions to a sleep state. Sleep states are identified with 32-bit wide
/// identifiers (`sleep_type`). The possible values for the identifiers are shown
/// in the table below:
///
/// | Type                    | Name           | Description    
/// |-------------------------|----------------|-------------------------------
/// | 0                       | SUSPEND_TO_RAM | This is a "suspend to RAM" sleep type, similar to ACPI’s S2 or S3. Entry requires all but the calling hart be in the HSM `STOPPED` state and all hart registers and CSRs saved to RAM.
/// | 0x00000001 - 0x7fffffff |                | Reserved for future use
/// | 0x80000000 - 0xffffffff |                | Platform-specific system sleep types
/// | > 0xffffffff            |                | Reserved                
///
/// The term "system" refers to the world-view of supervisor software. The
/// underlying SBI implementation may be provided by machine mode firmware or a
/// hypervisor.
///
/// The system suspend extension does not provide any way for supported sleep types
/// to be probed. Platforms are expected to specify their supported system sleep
/// types and per-type wake up devices in their hardware descriptions. The
/// `SUSPEND_TO_RAM` sleep type is the one exception, and its presence is implied
/// by that of the extension.
pub trait Susp: Send + Sync {
    /// Request the SBI implementation to put the system transitions to a sleep state.
    ///
    /// A return from a `system_suspend()` call implies an error and an error code
    /// will be in `sbiret.error`. A successful suspend and wake up, results in the
    /// hart which initiated the suspend, resuming from the `STOPPED` state. To resume,
    /// the hart will jump to supervisor-mode, at the address specified by `resume_addr`,
    /// with the specific register values described in the table below.
    ///
    /// | Register Name                                     | Register Value     
    /// | ------------------------------------------------- | ------------------
    /// | satp                                              | 0                  
    /// | sstatus.SIE                                       | 0                  
    /// | a0                                                | hartid             
    /// | a1                                                | `opaque` parameter
    /// All other registers remain in an undefined state.
    ///
    /// # Parameters
    ///
    /// The `resume_addr` parameter points to a runtime-specified physical address,
    /// where the hart can resume execution in supervisor-mode after a system suspend.
    ///
    /// *NOTE:* A single `usize` parameter is sufficient as `resume_addr`,
    /// because the hart will resume execution in supervisor-mode with the MMU off,
    /// hence `resume_addr` must be less than XLEN bits wide.
    ///
    /// The `opaque` parameter is an XLEN-bit value which will be set in the `a1`
    /// register when the hart resumes exectution at `resume_addr` after a
    /// system suspend.
    ///
    /// Besides ensuring all entry criteria for the selected sleep type are met, such
    /// as ensuring other harts are in the `STOPPED` state, the caller must ensure all
    /// power units and domains are in a state compatible with the selected sleep type.
    /// The preparation of the power units, power domains, and wake-up devices used for
    /// resumption from the system sleep state is platform specific and beyond the
    /// scope of this specification.
    ///
    /// When supervisor software is running inside a virtual machine, the SBI
    /// implementation is provided by a hypervisor. The system suspend will behave
    /// functionally the same as the native case, but might not result in any physical
    /// power changes.
    ///
    /// # Return value
    ///
    /// The possible return error codes returned in `SbiRet.error` are shown in the table below:
    ///
    /// | Error code                  | Description  
    /// | --------------------------- | -------------------
    /// | `SbiRet::success()`         | System has suspended and resumed successfully.
    /// | `SbiRet::invalid_param()`   | `sleep_type` is reserved or is platform-specific and unimplemented.
    /// | `SbiRet::not_supported()`   | `sleep_type` is not reserved and is implemented, but the platform does not support it due to one or more missing dependencies.
    /// | `SbiRet::invalid_address()` | `resume_addr` is not valid, possibly due to the following reasons: * It is not a valid physical address. * Executable access to the address is prohibited by a physical memory protection mechanism or H-extension G-stage for supervisor mode.
    /// | `SbiRet::failed()`          | The suspend request failed for unspecified or unknown other reasons.
    fn system_suspend(&self, sleep_type: u32, resume_addr: usize, opaque: usize) -> SbiRet;
}

impl<T: Susp> Susp for &T {
    #[inline]
    fn system_suspend(&self, sleep_type: u32, resume_addr: usize, opaque: usize) -> SbiRet {
        T::system_suspend(self, sleep_type, resume_addr, opaque)
    }
}
