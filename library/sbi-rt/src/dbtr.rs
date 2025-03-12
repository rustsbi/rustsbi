//! Debug Triggers Extension (EID #0x44425452 "DBTR")
//!
//! The RISC-V Sdtrig extension allows machine-mode software to directly
//! configure debug triggers which in-turn allows native (or hosted) debugging in machine-mode
//! without any external debugger. Unfortunately, the debug triggers are only accessible to
//! machine-mode.
//!
//! The SBI debug trigger extension defines a SBI based abstraction to provide native debugging
//! for supervisor-mode software such that it is:
//! 1. Suitable for the rich operating systems and hypervisors running in supervisor-mode.
//! 2. Allows Guest (VS-mode) and Hypervisor (HS-mode) to share debug triggers on a hart.
//!
//! Each hart on a RISC-V platform has a fixed number of debug triggers which is referred
//! to as `trig_max` in this SBI extension. Each debug trigger is assigned a logical index
//! called `trig_idx` by the SBI implementation where `-1 < trig_idx < trig_max`.

use crate::binary::{sbi_call_1, sbi_call_2, sbi_call_3};
use sbi_spec::binary::{SbiRet, SharedPtr};
use sbi_spec::dbtr::*;

/// Get the number of debug triggers on the calling hart which can support the trigger
/// configuration specified by `trig_tdata1` parameter.
///
/// This function always returns `SbiRet::success()` in `SbiRet.error`. It will return `trig_max`
/// in `SbiRet.value` when `trig_tdata1 == 0` otherwise it will return the number of matching
/// debug triggers in `SbiRet.value`.
#[doc(alias = "sbi_debug_num_triggers")]
#[inline]
pub fn debug_num_triggers(trig_tdata1: usize) -> SbiRet {
    sbi_call_1(EID_DBTR, NUM_TRIGGERS, trig_tdata1)
}

/// Set and enable the shared memory for debug trigger configuration on the calling hart.
///
/// If both `shmem.phys_addr_lo` and `shmem.phys_addr_hi` parameters are not all-ones bitwise then
/// `shmem.phys_addr_lo` specifies the lower XLEN bits and `shmem.phys_addr_hi` specifies
/// the upper XLEN bits of the shared memory physical base address. The
/// `shmem.phys_addr_lo` MUST be `(XLEN / 8)` bytes aligned and the size of shared
/// memory is assumed to be `trig_max * (XLEN / 2)` bytes.
///
/// If both `shmem.phys_addr_lo` and `shmem.phys_addr_hi` parameters are all-ones bitwise
/// then shared memory for debug trigger configuration is disabled.
///
/// The `flags` parameter is reserved for future use and MUST be zero.
///
/// # Return value
///
/// | Error code                    | Description
/// |:------------------------------|:---------------------------------
/// | `SbiRet::success()`           | Shared memory was set or cleared successfully.
/// | `SbiRet::invalid_param()`     | The `flags` parameter is not zero or the `shmem.phys_addr_lo` parameter is not `(XLEN / 8)` bytes aligned.
/// | `SbiRet::invalid_address()`   | The shared memory pointed to by the `shmem.phys_addr_lo` and `shmem.phys_addr_hi` parameters does not satisfy the requirements.
/// | `SbiRet::failed()`            | The request failed for unspecified or unknown other reasons.
#[doc(alias = "sbi_debug_set_shmem")]
#[inline]
pub fn debug_set_shmem(shmem: SharedPtr<u8>, flags: usize) -> SbiRet {
    sbi_call_3(
        EID_DBTR,
        SET_SHMEM,
        shmem.phys_addr_lo(),
        shmem.phys_addr_hi(),
        flags,
    )
}

/// Read the debug trigger state and configuration into shared memory for a range of
/// debug triggers specified by the `trig_idx_base` and `trig_count` parameters on the calling hart.
///
/// For each debug trigger with index `trig_idx_base + i` where `-1 < i < trig_count`, the
/// debug trigger state and configuration consisting of four XLEN-bit words are written in
/// little-endian format at `offset = i * (XLEN / 2)` of the shared memory as follows:
///
/// ```text
/// word[0] = `trig_state` written by the SBI implementation
/// word[1] = `trig_tdata1` written by the SBI implementation
/// word[2] = `trig_tdata2` written by the SBI implementation
/// word[3] = `trig_tdata3` written by the SBI implementation
/// ```
/// # Return value
///
/// | Error code              | Description
/// |:------------------------|:---------------------------------
/// | `SbiRet::success()`     | State and configuration of triggers read successfully.
/// | `SbiRet::no_shmem()`    | Shared memory for debug triggers is disabled.
/// | `SbiRet::bad_range()`   | Either `trig_idx_base >= trig_max` or `trig_idx_base + trig_count >= trig_max`.
#[doc(alias = "sbi_debug_read_triggers")]
#[inline]
pub fn debug_read_triggers(trig_idx_base: usize, trig_count: usize) -> SbiRet {
    sbi_call_2(EID_DBTR, READ_TRIGGERS, trig_idx_base, trig_count)
}

/// Install debug triggers based on an array of trigger configurations in the shared memory
/// of the calling hart. The `trig_idx` assigned to each installed trigger configuration is
/// written back in the shared memory.
///
/// The `trig_count` parameter represents the number of trigger configuration entries in
/// the shared memory at offset `0x0`.
///
/// The i'th trigger configuration at `offset = i * (XLEN / 2)` in the shared memory
/// consists of four consecutive XLEN-bit words in little-endian format which are
/// organized as follows:
///
/// ```text
/// word[0] = `trig_idx` written back by the SBI implementation
/// word[1] = `trig_tdata1` read by the SBI implementation
/// word[2] = `trig_tdata2` read by the SBI implementation
/// word[3] = `trig_tdata3` read by the SBI implementation
/// ```
///
/// Upon success, `SbiRet.value` is set to zero. Upon failure, `SbiRet.value` is set to the
/// array index of the failing trigger configuration.
///
/// # Return value
///
/// | Error code                  | Description
/// |:----------------------------|:---------------------------------
/// | `SbiRet::success()`         | Triggers installed successfully.
/// | `SbiRet::no_shmem()`        | Shared memory for debug triggers is disabled.
/// | `SbiRet::bad_range()`       | `trig_count >= trig_max`.
/// | `SbiRet::invalid_param()`   | One of the trigger configuration words `trig_tdata1`, `trig_tdata2`, or `trig_tdata3` has an invalid value.
/// | `SbiRet::failed()`          | Failed to assign `trig_idx` or HW debug trigger for one of the trigger configurations.
/// | `SbiRet::not_supported()`   | One of the trigger configuration can't be programmed due to unimplemented optional bits in `tdata1`, `tdata2`, or `tdata3` CSRs.
#[doc(alias = "sbi_debug_install_triggers")]
#[inline]
pub fn debug_install_triggers(trig_count: usize) -> SbiRet {
    sbi_call_1(EID_DBTR, INSTALL_TRIGGERS, trig_count)
}
/// Update already installed debug triggers based on a trigger configuration array in the
/// shared memory of the calling hart.
///
/// The `trig_count` parameter represents the number of trigger configuration entries in
/// the shared memory at offset `0x0`.
///
/// The i'th trigger configuration at `offset = i * (XLEN / 2)` in the shared memory
/// consists of four consecutive XLEN-bit words in little-endian format as follows:
///
/// ```text
/// word[0] = `trig_idx` read by the SBI implementation
/// word[1] = `trig_tdata1` read by the SBI implementation
/// word[2] = `trig_tdata2` read by the SBI implementation
/// word[3] = `trig_tdata3` read by the SBI implementation
/// ```
/// The SBI implementation MUST consider trigger configurations in the increasing order of
/// the array index and starting with array index `0`. To install a debug trigger for the
/// trigger configuration at array index `i` in the shared memory, the SBI implementation
/// MUST do the following:

/// - Map an unused HW debug trigger which matches the trigger configuration to an
///   an unused `trig_idx`.
/// - Save a copy of the `trig_tdata1.vs`, `trig_tdata1.vu`, `trig_tdata1.s`, and
///   `trig_tdata.u` bits in `trig_state`.
/// - Update the `tdata1`, `tdata2`, and `tdata3` CSRs of the HW debug trigger.
/// - Write `trig_idx` at `offset = i * (XLEN / 2)` in the shared memory.
///
/// Additionally for each trigger configuration chain in the shared memory, the SBI
/// implementation MUST assign contiguous `trig_idx` values and contiguous HW debug
/// triggers when installing the trigger configuration chain.
///
/// The last trigger configuration in the shared memory MUST not have `trig_tdata1.chain == 1`
/// for `trig_tdata1.type = 2 or 6` to prevent incomplete trigger configuration chain
/// in the shared memory.
///
/// The `SbiRet.value` is set to zero upon success or if shared memory is disabled whereas
/// `SbiRet.value` is set to the array index `i` of the failing trigger configuration upon
/// other failures.
///
/// # Return value
///
/// | Error code                  | Description
/// |:----------------------------|:---------------------------------
/// | `SbiRet::success()`         | Triggers updated successfully.
/// | `SbiRet::no_shmem()`        | Shared memory for debug triggers is disabled.
/// | `SbiRet::bad_range()`       | `trig_count >= trig_max`.
/// | `SbiRet::invalid_param()`   | One of the trigger configuration in the shared memory has an invalid of `trig_idx` (i.e. `trig_idx >= trig_max`), `trig_tdata1`, `trig_tdata2`, or `trig_tdata3`.
/// | `SbiRet::failed()`          | One of the trigger configurations has valid `trig_idx` but the corresponding debug trigger is not mapped to any HW debug trigger.
/// | `SbiRet::not_supported()`   | One of the trigger configuration can't be programmed due to unimplemented optional bits in `tdata1`, `tdata2`, or `tdata3` CSRs.
#[doc(alias = "sbi_debug_update_triggers")]
#[inline]
pub fn debug_update_triggers(trig_count: usize) -> SbiRet {
    sbi_call_1(EID_DBTR, UPDATE_TRIGGERS, trig_count)
}

/// Uninstall a set of debug triggers specified by the `trig_idx_base` and `trig_idx_mask`
/// parameters on the calling hart.
///
/// The `trig_idx_base` specifies the starting trigger index, while the `trig_idx_mask` is a
/// bitmask indicating which triggers, relative to the base, are to be uninstalled.
/// Each bit in the mask corresponds to a specific trigger, allowing for batch operations
/// on multiple triggers simultaneously.
///
/// For each debug trigger in the specified set of debug triggers, the SBI implementation MUST:
/// 1. Clear the `tdata1`, `tdata2`, and `tdata3` CSRs of the mapped HW debug trigger.
/// 2. Clear the `trig_state` of the debug trigger.
/// 3. Unmap and free the HW debug trigger and corresponding `trig_idx` for re-use in
///    the future trigger installations.
///
/// # Return value
///
/// | Error code                  | Description
/// |:----------------------------|:---------------------------------
/// | `SbiRet::success()`         | Triggers uninstalled successfully.
/// | `SbiRet::invalid_param()`   | One of the debug triggers with index `trig_idx` in the specified set of debug triggers either not mapped to any HW debug trigger OR has `trig_idx >= trig_max`.
#[doc(alias = "sbi_debug_uninstall_triggers")]
#[inline]
pub fn debug_uninstall_triggers(trig_idx_base: usize, trig_idx_mask: usize) -> SbiRet {
    sbi_call_2(EID_DBTR, UNINSTALL_TRIGGERS, trig_idx_base, trig_idx_mask)
}

/// Enable a set of debug triggers specified by the `trig_idx_base` and `trig_idx_mask`
/// parameters on the calling hart.
///
/// The `trig_idx_base` specifies the starting trigger index, while the `trig_idx_mask` is a
/// bitmask indicating which triggers, relative to the base, are to be enabled.
/// Each bit in the mask corresponds to a specific trigger, allowing for batch operations
/// on multiple triggers simultaneously.
///
/// To enable a debug trigger in the specified set of debug triggers, the SBI implementation
/// MUST restore the `vs`, `vu`, `s`, and `u` bits of the mapped HW debug trigger from their
/// saved copy in `trig_state`.
///
/// # Return value
///
/// | Error code                  | Description
/// |:----------------------------|:---------------------------------
/// | `SbiRet::success()`         | Triggers enabled successfully.
/// | `SbiRet::invalid_param()`   | One of the debug triggers with index `trig_idx` in the specified set of debug triggers either not mapped to any HW debug trigger OR has `trig_idx >= trig_max`.
#[doc(alias = "sbi_debug_enable_triggers")]
#[inline]
pub fn debug_enable_triggers(trig_idx_base: usize, trig_idx_mask: usize) -> SbiRet {
    sbi_call_2(EID_DBTR, ENABLE_TRIGGERS, trig_idx_base, trig_idx_mask)
}

/// Disable a set of debug triggers specified by the `trig_idx_base` and `trig_idx_mask`
/// parameters on the calling hart.
///
/// The `trig_idx_base` specifies the starting trigger index, while the `trig_idx_mask` is a
/// bitmask indicating which triggers, relative to the base, are to be disabled.
/// Each bit in the mask corresponds to a specific trigger, allowing for batch operations
/// on multiple triggers simultaneously.
///
/// To disable a debug trigger in the specified set of debug triggers, the SBI implementation
/// MUST clear the `vs`, `vu`, `s`, and `u` bits of the mapped HW debug trigger.
///
/// # Return value
///
/// | Error code                  | Description
/// |:----------------------------|:---------------------------------
/// | `SbiRet::success()`         | Triggers disabled successfully.
/// | `SbiRet::invalid_param()`   | One of the debug triggers with index `trig_idx` in the specified set of debug triggers either not mapped to any HW debug trigger OR has `trig_idx >= trig_max`.
#[doc(alias = "sbi_debug_disable_triggers")]
#[inline]
pub fn debug_disable_triggers(trig_idx_base: usize, trig_idx_mask: usize) -> SbiRet {
    sbi_call_2(EID_DBTR, DISABLE_TRIGGERS, trig_idx_base, trig_idx_mask)
}
