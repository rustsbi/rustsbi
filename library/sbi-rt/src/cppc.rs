//! Chapter 14. CPPC Extension (EID #0x43505043 "CPPC")

use crate::binary::sbi_call_1;
#[cfg(target_pointer_width = "64")]
use crate::binary::sbi_call_2;
#[cfg(target_pointer_width = "32")]
use crate::binary::sbi_call_3;
use sbi_spec::{
    binary::SbiRet,
    cppc::{EID_CPPC, PROBE, READ, READ_HI, WRITE},
};

/// Probe whether the CPPC register is implemented or not by the platform.
///
/// # Parameters
///
/// The `cppc_reg_id` parameter specifies the CPPC register ID.
///
/// # Return value
///
/// If the register is implemented, `SbiRet.value` will contain the register width.
/// If the register is not implemented, `SbiRet.value` will be set to 0.
///
/// The possible error codes returned in `SbiRet.error` are shown in the table below:
///
/// | Return code               | Description
/// |:--------------------------|:----------------------------------------------
/// | `SbiRet::success()`       | Probe completed successfully.
/// | `SbiRet::invalid_param()` | `cppc_reg_id` is reserved.
/// | `SbiRet::failed()`        | The probe request failed for unspecified or unknown other reasons.
///
/// This function is defined in RISC-V SBI Specification chapter 14.1.
#[inline]
pub fn cppc_probe(cppc_reg_id: u32) -> SbiRet {
    sbi_call_1(EID_CPPC, PROBE, cppc_reg_id as _)
}

/// Read the CPPC register identified by given `cppc_reg_id`.
///
/// # Parameters
///
/// The `cppc_reg_id` parameter specifies the CPPC register ID.
///
/// # Return value
///
/// `SbiRet.value` will contain the register value. When supervisor mode XLEN is 32, the `SbiRet.value`
/// will only contain the lower 32 bits of the CPPC register value.
///
/// The possible error codes returned in `SbiRet.error` are shown in the table below:
///
/// | Return code               | Description
/// |:--------------------------|:----------------------------------------------
/// | `SbiRet::success()`       | Read completed successfully.
/// | `SbiRet::invalid_param()` | `cppc_reg_id` is reserved.
/// | `SbiRet::not_supported()` | `cppc_reg_id` is not implemented by the platform.
/// | `SbiRet::denied()`        | `cppc_reg_id` is a write-only register.
/// | `SbiRet::failed()`        | The read request failed for unspecified or unknown other reasons.
///
/// This function is defined in RISC-V SBI Specification chapter 14.2.
#[inline]
pub fn cppc_read(cppc_reg_id: u32) -> SbiRet {
    sbi_call_1(EID_CPPC, READ, cppc_reg_id as _)
}

/// Read the upper 32-bit value of the CPPC register identified by `cppc_reg_id`.
///
/// # Parameters
///
/// The `cppc_reg_id` parameter specifies the CPPC register ID.
///
/// # Return value
///
/// `SbiRet.value` will contain the upper 32 bits of the register value. This function always
/// returns zero in `SbiRet.value` when supervisor mode XLEN is 64 or higher.
///
/// The possible error codes returned in `SbiRet.error` are shown in the table below:
///
/// | Return code               | Description
/// |:--------------------------|:----------------------------------------------
/// | `SbiRet::success()`       | Read completed successfully.
/// | `SbiRet::invalid_param()` | `cppc_reg_id` is reserved.
/// | `SbiRet::not_supported()` | `cppc_reg_id` is not implemented by the platform.
/// | `SbiRet::denied()`        | `cppc_reg_id` is a write-only register.
/// | `SbiRet::failed()`        | The read operation request failed for unspecified or unknown other reasons.
///
/// This function is defined in RISC-V SBI Specification chapter 14.3.
#[inline]
pub fn cppc_read_hi(cppc_reg_id: u32) -> SbiRet {
    sbi_call_1(EID_CPPC, READ_HI, cppc_reg_id as _)
}

/// Write 64-bit value to the CPPC register identified by given `cppc_reg_id`.
///
/// # Parameters
///
/// The `cppc_reg_id` parameter specifies the CPPC register ID.
///
/// The `value` parameter specifies the value to be written to the register.
///
/// # Return value
///
/// The possible error codes returned in `SbiRet.error` are shown in the table below:
///
/// | Return code               | Description
/// |:--------------------------|:----------------------------------------------
/// | `SbiRet::success()`       | Write completed successfully.
/// | `SbiRet::invalid_param()` | `cppc_reg_id` is reserved.
/// | `SbiRet::not_supported()` | `cppc_reg_id` is not implemented by the platform.
/// | `SbiRet::denied()`        | `cppc_reg_id` is a read-only register.
/// | `SbiRet::failed()`        | The write operation request failed for unspecified or unknown other reasons.
///
/// This function is defined in RISC-V SBI Specification chapter 14.4.
#[inline]
pub fn cppc_write(cppc_reg_id: u32, value: u64) -> SbiRet {
    match () {
        #[cfg(target_pointer_width = "32")]
        () => sbi_call_3(
            EID_CPPC,
            WRITE,
            cppc_reg_id as _,
            value as _,
            (value >> 32) as _,
        ),
        #[cfg(target_pointer_width = "64")]
        () => sbi_call_2(EID_CPPC, WRITE, cppc_reg_id as _, value as _),
    }
}
