use sbi_spec::binary::SbiRet;

/// System Reset extension.
///
/// Provides a function that allows the supervisor software to request system-level reboot or shutdown.
///
/// The term "system" refers to the world-view of supervisor software and the underlying SBI implementation
/// could be machine mode firmware or hypervisor.
///
/// Ref: [Section 9, RISC-V Supervisor Binary Interface Specification](https://github.com/riscv-non-isa/riscv-sbi-doc/blob/master/riscv-sbi.adoc#9-system-reset-extension-eid-0x53525354-srst)
pub trait Reset {
    /// Reset the system based on provided `reset_type` and `reset_reason`.
    ///
    /// This is a synchronous call and does not return if it succeeds.
    ///
    /// # Warm reboot and cold reboot
    ///
    /// When supervisor software is running natively, the SBI implementation is machine mode firmware.
    /// In this case, shutdown is equivalent to physical power down of the entire system, and
    /// cold reboot is equivalent to a physical power cycle of the entire system.
    /// Further, warm reboot is equivalent to a power cycle of the main processor and parts of the system
    /// but not the entire system.
    ///
    /// For example, on a server class system with a BMC (board management controller),
    /// a warm reboot will not power cycle the BMC
    /// whereas a cold reboot will definitely power cycle the BMC.
    ///
    /// When supervisor software is running inside a virtual machine,
    /// the SBI implementation is a hypervisor.
    /// The shutdown,
    /// cold reboot and warm reboot will behave functionally the same as the native case but might
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
}

impl<T: Reset> Reset for &T {
    #[inline]
    fn system_reset(&self, reset_type: u32, reset_reason: u32) -> SbiRet {
        T::system_reset(self, reset_type, reset_reason)
    }
}
