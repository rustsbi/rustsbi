//! Chapter 11. Performance Monitoring Unit Extension (EID #0x504D55 "PMU")

use crate::binary::{sbi_call_0, sbi_call_1, sbi_call_3};

use sbi_spec::{
    binary::SbiRet,
    pmu::{
        COUNTER_CONFIG_MATCHING, COUNTER_FW_READ, COUNTER_FW_READ_HI, COUNTER_GET_INFO,
        COUNTER_START, COUNTER_STOP, EID_PMU, NUM_COUNTERS,
    },
};

/// Returns the number of counters, both hardware and firmware.
///
/// This call would always succeed without returning any error.
///
/// This function is defined in RISC-V SBI Specification chapter 11.5.
#[inline]
pub fn pmu_num_counters() -> usize {
    sbi_call_0(EID_PMU, NUM_COUNTERS).value
}

/// Get details about the specified counter.
///
/// The value returned includes details such as underlying CSR number, width of the counter,
/// type of counter (hardware or firmware), etc.
///
/// The `counter_info` returned by this SBI call is encoded as follows:
///
/// ```text
///     counter_info[11:0] = CSR; // (12bit CSR number)
///     counter_info[17:12] = Width; // (One less than number of bits in CSR)
///     counter_info[XLEN-2:18] = Reserved; // Reserved for future use
///     counter_info[XLEN-1] = Type; // (0 = hardware and 1 = firmware)
/// ```
/// If `counter_info.type` == `1` then `counter_info.csr` and `counter_info.width` should be ignored.
///
/// This function is defined in RISC-V SBI Specification chapter 11.6.
///
/// # Return value
///
/// Returns the `counter_info` described above in `SbiRet.value`.
///
/// The possible return error codes returned in `SbiRet.error` are shown in the table below:    
///
/// | Return code               | Description
/// |:--------------------------|:----------------------------------------------
/// | `SbiRet::success()`       | `counter_info` read successfully.
/// | `SbiRet::invalid_param()` | `counter_idx` points to an invalid counter.
///
/// This function is defined in RISC-V SBI Specification chapter 11.6.
#[inline]
pub fn pmu_counter_get_info(counter_idx: usize) -> SbiRet {
    sbi_call_1(EID_PMU, COUNTER_GET_INFO, counter_idx)
}

/// Find and configure a counter from a set of counters.
///
/// The counters to be found and configured should not be started (or enabled)
/// and should be able to monitor the specified event.
///
/// # Parameters
///
/// The `counter_idx_base` and `counter_idx_mask` parameters represent the set of counters,
/// whereas the `event_idx` represent the event to be monitored
/// and `event_data` represents any additional event configuration.
///
/// The `config_flags` parameter represents additional configuration and filter flags of the counter.
/// The bit definitions of the `config_flags` parameter are shown in the table below:
///
/// | Flag Name                    | Bits       | Description
/// |:-----------------------------|:-----------|:------------
/// | SBI_PMU_CFG_FLAG_SKIP_MATCH  | 0:0        | Skip the counter matching
/// | SBI_PMU_CFG_FLAG_CLEAR_VALUE | 1:1        | Clear (or zero) the counter value in counter configuration
/// | SBI_PMU_CFG_FLAG_AUTO_START  | 2:2        | Start the counter after configuring a matching counter
/// | SBI_PMU_CFG_FLAG_SET_VUINH   | 3:3        | Event counting inhibited in VU-mode
/// | SBI_PMU_CFG_FLAG_SET_VSINH   | 4:4        | Event counting inhibited in VS-mode
/// | SBI_PMU_CFG_FLAG_SET_UINH    | 5:5        | Event counting inhibited in U-mode
/// | SBI_PMU_CFG_FLAG_SET_SINH    | 6:6        | Event counting inhibited in S-mode
/// | SBI_PMU_CFG_FLAG_SET_MINH    | 7:7        | Event counting inhibited in M-mode
/// | _RESERVED_                   | 8:(XLEN-1) | _All non-zero values are reserved for future use._
///
/// *NOTE:* When *SBI_PMU_CFG_FLAG_SKIP_MATCH* is set in `config_flags`, the
/// SBI implementation will unconditionally select the first counter from the
/// set of counters specified by the `counter_idx_base` and `counter_idx_mask`.
///
/// *NOTE:* The *SBI_PMU_CFG_FLAG_AUTO_START* flag in `config_flags` has no
/// impact on the value of the counter.
///
/// *NOTE:* The `config_flags[3:7]` bits are event filtering hints so these
/// can be ignored or overridden by the SBI implementation for security concerns
/// or due to lack of event filtering support in the underlying RISC-V platform.
///
/// # Return value
///
/// Returns the `counter_idx` in `sbiret.value` upon success.
///
/// In case of failure, the possible error codes returned in `sbiret.error` are shown in the table below:    
///
/// | Return code               | Description
/// |:--------------------------|:----------------------------------------------
/// | `SbiRet::success()`       | counter found and configured successfully.
/// | `SbiRet::invalid_param()` | set of counters has an invalid counter.
/// | `SbiRet::not_supported()` | none of the counters can monitor specified event.
///
/// This function is defined in RISC-V SBI Specification chapter 11.7.
#[inline]
pub fn pmu_counter_config_matching<T>(
    counter_idx_base: usize,
    counter_idx_mask: usize,
    config_flags: T,
    event_idx: usize,
    event_data: u64,
) -> SbiRet
where
    T: ConfigFlags,
{
    match () {
        #[cfg(target_pointer_width = "32")]
        () => crate::binary::sbi_call_6(
            EID_PMU,
            COUNTER_CONFIG_MATCHING,
            counter_idx_base,
            counter_idx_mask,
            config_flags.raw(),
            event_idx,
            event_data as _,
            (event_data >> 32) as _,
        ),
        #[cfg(target_pointer_width = "64")]
        () => crate::binary::sbi_call_5(
            EID_PMU,
            COUNTER_CONFIG_MATCHING,
            counter_idx_base,
            counter_idx_mask,
            config_flags.raw(),
            event_idx,
            event_data as _,
        ),
    }
}

/// Start or enable a set of counters on the calling hart with the specified initial value.
///
/// # Parameters
///
/// The `counter_idx_base` and `counter_idx_mask` parameters represent the set of counters.
/// whereas the `initial_value` parameter specifies the initial value of the counter.
///
/// The bit definitions of the `start_flags` parameter are shown in the table below:
///
/// | Flag Name                    | Bits       | Description
/// |:-----------------------------|:-----------|:------------
/// | SBI_PMU_START_SET_INIT_VALUE | 0:0        | Set the value of counters based on the `initial_value` parameter.
/// | _RESERVED_                   | 1:(XLEN-1) | _All non-zero values are reserved for future use._
///
/// *NOTE*: When `SBI_PMU_START_SET_INIT_VALUE` is not set in `start_flags`, the value of counter will
/// not be modified, and event counting will start from the current value of counter.
///
/// # Return value
///
/// The possible return error codes returned in `SbiRet.error` are shown in the table below:    
///
/// | Return code                 | Description
/// |:----------------------------|:----------------------------------------------
/// | `SbiRet::success()`         | counter started successfully.
/// | `SbiRet::invalid_param()`   | some of the counters specified in parameters are invalid.
/// | `SbiRet::already_started()` | some of the counters specified in parameters are already started.
///
/// This function is defined in RISC-V SBI Specification chapter 11.8.
#[inline]
pub fn pmu_counter_start<T>(
    counter_idx_base: usize,
    counter_idx_mask: usize,
    start_flags: T,
    initial_value: u64,
) -> SbiRet
where
    T: StartFlags,
{
    match () {
        #[cfg(target_pointer_width = "32")]
        () => crate::binary::sbi_call_5(
            EID_PMU,
            COUNTER_START,
            counter_idx_base,
            counter_idx_mask,
            start_flags.raw(),
            initial_value as _,
            (initial_value >> 32) as _,
        ),
        #[cfg(target_pointer_width = "64")]
        () => crate::binary::sbi_call_4(
            EID_PMU,
            COUNTER_START,
            counter_idx_base,
            counter_idx_mask,
            start_flags.raw(),
            initial_value as _,
        ),
    }
}

/// Stop or disable a set of counters on the calling hart.
///
/// # Parameters
///
/// The `counter_idx_base` and `counter_idx_mask` parameters represent the set of counters.
/// The bit definitions of the `stop_flags` parameter are shown in the table below:
///
/// | Flag Name               | Bits       | Description
/// |:------------------------|:-----------|:------------
/// | SBI_PMU_STOP_FLAG_RESET | 0:0        | Reset the counter to event mapping.
/// | _RESERVED_              | 1:(XLEN-1) | *All non-zero values are reserved for future use.*
///
/// # Return value
///
/// The possible return error codes returned in `SbiRet.error` are shown in the table below:    
///
/// | Return code                 | Description
/// |:----------------------------|:----------------------------------------------
/// | `SbiRet::success()`         | counter stopped successfully.
/// | `SbiRet::invalid_param()`   | some of the counters specified in parameters are invalid.
/// | `SbiRet::already_stopped()` | some of the counters specified in parameters are already stopped.
///
/// This function is defined in RISC-V SBI Specification chapter 11.9.
#[inline]
pub fn pmu_counter_stop<T>(
    counter_idx_base: usize,
    counter_idx_mask: usize,
    stop_flags: T,
) -> SbiRet
where
    T: StopFlags,
{
    sbi_call_3(
        EID_PMU,
        COUNTER_STOP,
        counter_idx_base,
        counter_idx_mask,
        stop_flags.raw(),
    )
}

/// Provide the current value of a firmware counter.
///
/// On RV32 systems, the `SbiRet.value` will only contain the lower 32 bits from the current
/// value of the firmware counter.
///
/// # Parameters
///
/// This function should be only used to read a firmware counter. It will return an error
/// when a user provides a hardware counter in `counter_idx` parameter.
///
/// # Return value
///
/// The possible return error codes returned in `SbiRet.error` are shown in the table below:    
///
/// | Return code               | Description
/// |:--------------------------|:----------------------------------------------
/// | `SbiRet::success()`       | firmware counter read successfully.
/// | `SbiRet::invalid_param()` | `counter_idx` points to a hardware counter or an invalid counter.
///
/// This function is defined in RISC-V SBI Specification chapter 11.10.
#[inline]
pub fn pmu_counter_fw_read(counter_idx: usize) -> SbiRet {
    sbi_call_1(EID_PMU, COUNTER_FW_READ, counter_idx)
}

/// Provide the upper 32 bits from the value of a firmware counter.
///
/// This function always returns zero in `SbiRet.value` for RV64 (or higher) systems.
///
/// # Return value
///
/// The possible return error codes returned in `SbiRet.error` are shown in the table below:
///
/// | Return code               | Description
/// |:--------------------------|:----------------------------------------------
/// | `SbiRet::success()`       | firmware counter read successfully.
/// | `SbiRet::invalid_param()` | `counter_idx` points to a hardware counter or an invalid counter.
///
/// This function is defined in RISC-V SBI Specification chapter 11.11.
#[inline]
pub fn pmu_counter_fw_read_hi(counter_idx: usize) -> SbiRet {
    sbi_call_1(EID_PMU, COUNTER_FW_READ_HI, counter_idx)
}

/// Flags to configure performance counter.
pub trait ConfigFlags {
    /// Get a raw value to pass to SBI environment.
    fn raw(&self) -> usize;
}

#[cfg(feature = "integer-impls")]
impl ConfigFlags for usize {
    #[inline]
    fn raw(&self) -> usize {
        *self
    }
}

/// Flags to start performance counter.
pub trait StartFlags {
    /// Get a raw value to pass to SBI environment.
    fn raw(&self) -> usize;
}

#[cfg(feature = "integer-impls")]
impl StartFlags for usize {
    #[inline]
    fn raw(&self) -> usize {
        *self
    }
}

/// Flags to stop performance counter.
pub trait StopFlags {
    /// Get a raw value to pass to SBI environment.
    fn raw(&self) -> usize;
}

#[cfg(feature = "integer-impls")]
impl StopFlags for usize {
    #[inline]
    fn raw(&self) -> usize {
        *self
    }
}
