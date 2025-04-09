//! Chapter 18. SBI Firmware Features Extension (EID #0x46574654 "FWFT").

use crate::binary::{sbi_call_1, sbi_call_3};
use sbi_spec::{
    binary::SbiRet,
    fwft::{EID_FWFT, GET, SET},
};

/// Set the configuration value of a specific firmware feature.
///
/// # Parameters
///
/// - `feature`: The identifier of the feature to set.
/// - `value`: The value to set for the feature.
/// - `flags`: Flags to modify the behavior of the set operation.
///
/// # Return value
///
/// A successful return results in the requested firmware feature to be set according to the `value` and `flags` parameters. In case of failure, `feature` value is not modified and the possible error codes returned in `SbiRet.error` are shown in the table below:
///
/// | Error code                  | Description
/// |:----------------------------|:---------------------------------
/// | `SbiRet::success()`         | `feature` was set successfully.
/// | `SbiRet::not_supported()`   | `feature` is not reserved and valid, but the platform does not support it due to one or more missing dependencies (Hardware or SBI implementation).
/// | `SbiRet::invalid_param()`   | Provided `value` or `flags` parameter is invalid.
/// | `SbiRet::denied()`          | `feature` set operation failed because either it was denied by the SBI implementation, or`feature` is reserved or is platform-specific and unimplemented.
/// | `SbiRet::denied_locked()`   | `feature` set operation failed because the `feature` is locked.
/// | `SbiRet::failed()`          | The set operation failed for unspecified or unknown other reasons.
///
/// This function is defined in RISC-V SBI Specification chapter 18.1.
#[inline]
#[doc(alias = "sbi_fwft_set")]
pub fn fwft_set(feature: u32, value: usize, flags: usize) -> SbiRet {
    sbi_call_3(EID_FWFT, SET, feature as _, value, flags)
}

/// Get the configuration value of a specific firmware feature.
///
/// # Parameters
///
/// - `feature`: The identifier of the feature to get.
///
/// # Return value
///
/// A successful return results in the firmware feature configuration value to be returned in `SbiRet.value`. In case of failure, the content of `SbiRet.value` is zero and the possible error codes returned in `SbiRet.error` are shown in the table below:
///
/// | Error code                  | Description
/// |:----------------------------|:---------------------------------
/// | `SbiRet::success()`         | Feature status was retrieved successfully.
/// | `SbiRet::not_supported()`   | `feature` is not reserved and valid, but the platform does not support it due to one or more missing dependencies (Hardware or SBI implementation).
/// | `SbiRet::denied()`          | `feature` is reserved or is platform-specific and unimplemented.
/// | `SbiRet::failed()`          | The get operation failed for unspecified or unknown other reasons.
///
/// This function is defined in RISC-V SBI Specification chapter 18.2.
#[inline]
#[doc(alias = "sbi_fwft_get")]
pub fn fwft_get(feature: u32) -> SbiRet {
    sbi_call_1(EID_FWFT, GET, feature as _)
}
