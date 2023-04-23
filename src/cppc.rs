use sbi_spec::binary::SbiRet;

/// CPPC Extension
///
/// ACPI defines the Collaborative Processor Performance Control (CPPC) mechanism,
/// which is an abstract and flexible mechanism for the supervisor-mode
/// power-management software to collaborate with an entity in the platform to
/// manage the performance of the processors.
///
/// The SBI CPPC extension provides an abstraction to access the CPPC registers
/// through SBI calls. The CPPC registers can be memory locations shared with a
/// separate platform entity such as a BMC. Even though CPPC is defined in the ACPI
/// specification, it may be possible to implement a CPPC driver based on
/// Device Tree.
///
/// The table below defines 32-bit identifiers for all CPPC registers
/// to be used by the SBI CPPC functions. The first half of the 32-bit register
/// space corresponds to the registers as defined by the ACPI specification.
/// The second half provides the information not defined in the ACPI specification,
/// but is additionally required by the supervisor-mode power-management software.
///
/// | Register ID             | Register                              | Bit Width | Attribute    | Description               
/// | ----------------------- | ------------------------------------- | --------- | ------------ | ---------------------------
/// | 0x00000000              | HighestPerformance                    | 32        | Read-only    | ACPI Spec 6.5: 8.4.6.1.1.1  
/// | 0x00000001              | NominalPerformance                    | 32        | Read-only    | ACPI Spec 6.5: 8.4.6.1.1.2   
/// | 0x00000002              | LowestNonlinearPerformance            | 32        | Read-only    | ACPI Spec 6.5: 8.4.6.1.1.4
/// | 0x00000003              | LowestPerformance                     | 32        | Read-only    | ACPI Spec 6.5: 8.4.6.1.1.5
/// | 0x00000004              | GuaranteedPerformanceRegister         | 32        | Read-only    | ACPI Spec 6.5: 8.4.6.1.1.6
/// | 0x00000005              | DesiredPerformanceRegister            | 32        | Read / Write | ACPI Spec 6.5: 8.4.6.1.2.3
/// | 0x00000006              | MinimumPerformanceRegister            | 32        | Read / Write | ACPI Spec 6.5: 8.4.6.1.2.2
/// | 0x00000007              | MaximumPerformanceRegister            | 32        | Read / Write | ACPI Spec 6.5: 8.4.6.1.2.1
/// | 0x00000008              | PerformanceReductionToleranceRegister | 32        | Read / Write | ACPI Spec 6.5: 8.4.6.1.2.4
/// | 0x00000009              | TimeWindowRegister                    | 32        | Read / Write | ACPI Spec 6.5: 8.4.6.1.2.5
/// | 0x0000000A              | CounterWraparoundTime                 | 32 / 64   | Read-only    | ACPI Spec 6.5: 8.4.6.1.3.1
/// | 0x0000000B              | ReferencePerformanceCounterRegister   | 32 / 64   | Read-only    | ACPI Spec 6.5: 8.4.6.1.3.1
/// | 0x0000000C              | DeliveredPerformanceCounterRegister   | 32 / 64   | Read-only    | ACPI Spec 6.5: 8.4.6.1.3.1
/// | 0x0000000D              | PerformanceLimitedRegister            | 32        | Read / Write | ACPI Spec 6.5: 8.4.6.1.3.2
/// | 0x0000000E              | CPPCEnableRegister                    | 32        | Read / Write | ACPI Spec 6.5: 8.4.6.1.4
/// | 0x0000000F              | AutonomousSelectionEnable             | 32        | Read / Write | ACPI Spec 6.5: 8.4.6.1.5
/// | 0x00000010              | AutonomousActivityWindowRegister      | 32        | Read / Write | ACPI Spec 6.5: 8.4.6.1.6
/// | 0x00000011              | EnergyPerformancePreferenceRegister   | 32        | Read / Write | ACPI Spec 6.5: 8.4.6.1.7  
/// | 0x00000012              | ReferencePerformance                  | 32        | Read-only    | ACPI Spec 6.5: 8.4.6.1.1.3  
/// | 0x00000013              | LowestFrequency                       | 32        | Read-only    | ACPI Spec 6.5: 8.4.6.1.1.7
/// | 0x00000014              | NominalFrequency                      | 32        | Read-only    | ACPI Spec 6.5: 8.4.6.1.1.7
/// | 0x00000015 - 0x7FFFFFFF |                                       |           |              | Reserved for future use.     
/// | 0x80000000              | TransitionLatency                     | 32        | Read-only    | Provides the maximum (worst-case) performance state transition latency in nanoseconds.
/// | 0x80000001 - 0xFFFFFFFF |                                       |           |              | Reserved for future use.   
///
pub trait Cppc: Send + Sync {
    /// Probe whether the CPPC register as specified by the `reg_id` parameter
    /// is implemented or not by the platform.
    ///
    /// # Return value
    ///
    /// If the register is implemented, `SbiRet.value` will contain the register
    /// width. If the register is not implemented, `SbiRet.value` will be set to 0.
    ///
    /// The possible return error codes returned in `SbiRet.error` are shown in the table below:
    ///
    /// | Error code                | Description   
    /// | ------------------------- | ---------------
    /// | `SbiRet::success()`       | Probe completed successfully.
    /// | `SbiRet::invalid_param()` | `reg_id` is reserved.
    /// | `SbiRet::failed()`        | The probe request failed for unspecified or unknown other reasons.
    fn probe(&self, reg_id: u32) -> SbiRet;
    /// Reads the register as specified in the `reg_id` parameter.
    ///
    /// # Return value
    ///
    /// Returns the value of the register in `SbiRet.value`. When supervisor mode XLEN is 32,
    /// the `SbiRet.value` will only contain the lower 32 bits of the CPPC register value.
    ///
    /// The possible return error codes returned in `SbiRet.error` are shown in the table below:
    ///
    /// | Error code                | Description
    /// | ------------------------- | -------------------
    /// | `SbiRet::success()`       | Read completed successfully.
    /// | `SbiRet::invalid_param()` | `reg_id` is reserved.
    /// | `SbiRet::not_supported()` | `reg_id` is not implemented by the platform.
    /// | `SbiRet::denied()`        | `reg_id` is a write-only register.  
    /// | `SbiRet::failed()`        | The read request failed for unspecified or unknown other reasons.
    fn read(&self, reg_id: u32) -> SbiRet;
    /// Reads the upper 32-bit value of the register specified in the `reg_id`
    /// parameter.
    ///
    /// # Return value
    ///
    /// Returns the value of the register in `SbiRet.value`. This function always
    /// returns zero in `SbiRet.value` when supervisor mode XLEN is 64 or higher.
    ///
    /// The possible return error codes returned in `SbiRet.error` are shown in the table below:
    ///
    /// | Error code                | Description
    /// | ------------------------- | -------------------
    /// | `SbiRet::success()`       | Read completed successfully.
    /// | `SbiRet::invalid_param()` | `reg_id` is reserved.
    /// | `SbiRet::not_supported()` | `reg_id` is not implemented by the platform.
    /// | `SbiRet::denied()`        | `reg_id` is a write-only register.  
    /// | `SbiRet::failed()`        | The read request failed for unspecified or unknown other reasons.
    fn read_hi(&self, reg_id: u32) -> SbiRet;
    /// Writes the value passed in the `val` parameter to the register as
    /// specified in the `reg_id` parameter.
    ///
    /// # Return value
    ///
    /// The possible return error codes returned in `SbiRet.error` are shown in the table below:
    ///
    /// | Error code                | Description        
    /// | ------------------------- | -------------------
    /// | `SbiRet::success()`       | Write completed successfully.  
    /// | `SbiRet::invalid_param()` | `reg_id` is reserved.       
    /// | `SbiRet::not_supported()` | `reg_id` is not implemented by the platform.  
    /// | `SbiRet::denied()`        | `reg_id` is a read-only register.
    /// | `SbiRet::failed()`        | The write request failed for unspecified or unknown other reasons.
    fn write(&self, reg_id: u32, val: u64) -> SbiRet;
}

impl<T: Cppc> Cppc for &T {
    fn probe(&self, reg_id: u32) -> SbiRet {
        T::probe(self, reg_id)
    }
    fn read(&self, reg_id: u32) -> SbiRet {
        T::read(self, reg_id)
    }
    fn read_hi(&self, reg_id: u32) -> SbiRet {
        T::read_hi(self, reg_id)
    }
    fn write(&self, reg_id: u32, val: u64) -> SbiRet {
        T::write(self, reg_id, val)
    }
}
