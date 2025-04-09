//! Chapter 17. Supervisor Software Events Extension (EID #0x535345 "SSE").

use crate::binary::{sbi_call_0, sbi_call_1, sbi_call_2, sbi_call_3, sbi_call_5};
use sbi_spec::{
    binary::{SbiRet, SharedPtr},
    sse::*,
};

/// Read a range of event attribute values from a software event.
///
/// The `event_id` parameter specifies the software event ID whereas `base_attr_id`
/// and `attr_count` parameters specifies the range of event attribute IDs.
///
/// The event attribute values are written to a output shared memory which is specified
/// by the `output` parameter where:
///
/// - The `output` parameter MUST be `XLEN / 8` bytes aligned
/// - The size of output shared memory is assumed to be `(XLEN / 8) * attr_count`
/// - The value of event attribute with ID `base_attr_id + i` should be read from offset `(XLEN / 8) * (base_attr_id + i)`
///
/// The possible error codes returned in sbiret.error are shown below.
///
/// # Return value
///
/// | Error code                  | Description
/// |:----------------------------|:---------------------------------
/// | `SbiRet::success()`         | Event attribute values read successfully.
/// | `SbiRet::not_supported()`   | `event_id` is not reserved and valid, but the platform does not support it due to one or more missing dependencies (Hardware or SBI implementation).
/// | `SbiRet::invalid_param()`   | `event_id` is invalid or `attr_count` is zero.
/// | `SbiRet::bad_range()`       | One of the event attribute IDs in the range specified by `base_attr_id` and `attr_count` is reserved.
/// | `SbiRet::invalid_address()` | The shared memory pointed to by the `output` parameter does not satisfy the requirements.
/// | `SbiRet::failed()`          | The read failed for unspecified or unknown other reasons.
#[doc(alias = "sbi_sse_read_attrs")]
#[inline]
pub fn sse_read_attrs(
    event_id: u32,
    base_attr_id: u32,
    attr_count: u32,
    output: SharedPtr<u8>,
) -> SbiRet {
    sbi_call_5(
        EID_SSE,
        READ_ATTRS,
        event_id as _,
        base_attr_id as _,
        attr_count as _,
        output.phys_addr_lo(),
        output.phys_addr_hi(),
    )
}

/// Write a range of event attribute values to a software event.
///
/// The event_id parameter specifies the software event ID whereas `base_attr_id` and
/// `attr_count` parameters specifies the range of event attribute IDs.
///
/// The event attribute values are read from a input shared memory which is specified
/// by the `input` parameter where:
///
/// - The `input` parameter MUST be `XLEN / 8` bytes aligned
/// - The size of input shared memory is assumed to be `(XLEN / 8) * attr_count`
/// - The value of event attribute with ID `base_attr_id + i` should be read from offset `(XLEN / 8) * (base_attr_id + i)`
///
/// For local events, the event attributes are updated only for the calling hart.
/// For global events, the event attributes are updated for all the harts.
/// The possible error codes returned in sbiret.error are shown below.
///
/// # Return value
///
/// | Error code                  | Description
/// |:----------------------------|:---------------------------------
/// | `SbiRet::success()`         | Event attribute values written successfully.
/// | `SbiRet::not_supported()`   | `event_id` is not reserved and valid, but the platform does not support it due to one or more missing dependencies (Hardware or SBI implementation).
/// | `SbiRet::invalid_param()`   | `event_id` is invalid or `attr_count` is zero.
/// | `SbiRet::bad_range()`       | One of the event attribute IDs in the range specified by `base_attr_id` and `attr_count` is reserved or is read-only.
/// | `SbiRet::invalid_address()` | The shared memory pointed to by the `input` parameter does not satisfy the requirements.
/// | `SbiRet::failed()`          | The write failed for unspecified or unknown other reasons.
#[doc(alias = "sbi_sse_write_attrs")]
#[inline]
pub fn sse_write_attrs(
    event_id: u32,
    base_attr_id: u32,
    attr_count: u32,
    input: SharedPtr<u8>,
) -> SbiRet {
    sbi_call_5(
        EID_SSE,
        WRITE_ATTRS,
        event_id as _,
        base_attr_id as _,
        attr_count as _,
        input.phys_addr_lo(),
        input.phys_addr_hi(),
    )
}

/// Register an event handler for the software event.
///
/// The `event_id` parameter specifies the event ID for which an event handler is being registered.
/// The `handler_entry_pc` parameter MUST be 2-bytes aligned and specifies the `ENTRY_PC` event
/// attribute of the software event whereas the `handler_entry_arg` parameter specifies the
/// `ENTRY_ARG` event attribute of the software event.
///
/// For local events, the event is registered only for the calling hart.
/// For global events, the event is registered for all the harts.
///
/// The event MUST be in `UNUSED` state otherwise this function will fail.
///
/// Upon success, the event state moves from `UNUSED` to `REGISTERED`. In case of an error, possible error codes are listed below.
///
/// # Return value
///
/// | Error code                  | Description
/// |:----------------------------|:---------------------------------
/// | `SbiRet::success()`         | Event handler is registered successfully.
/// | `SbiRet::not_supported()`   | `event_id` is not reserved and valid, but the platform does not support it due to one or more missing dependencies (Hardware or SBI implementation).
/// | `SbiRet::invalid_state()`   | `event_id` is valid but the event is not in `UNUSED` state.
/// | `SbiRet::invalid_param()`   | `event_id` is invalid or `handler_entry_pc` is not 2-bytes aligned.    
#[doc(alias = "sbi_sse_register")]
#[inline]
pub fn sse_register(event_id: u32, handler_entry_pc: usize, handler_entry_arg: usize) -> SbiRet {
    sbi_call_3(
        EID_SSE,
        REGISTER,
        event_id as _,
        handler_entry_pc,
        handler_entry_arg,
    )
}

/// Unregister the event handler for given `event_id`.
///
/// For local events, the event is unregistered only for the calling hart.
/// For global events, the event is unregistered for all the harts.
///
/// The event MUST be in `REGISTERED` state otherwise this function will fail.
///
/// Upon success, the event state moves from `REGISTERED` to `UNUSED`. In case of an error, possible error codes are listed below.
///
/// # Return value
///
/// | Error code                  | Description
/// |:----------------------------|:---------------------------------
/// | `SbiRet::success()`         | Event handler is unregistered successfully.
/// | `SbiRet::not_supported()`   | `event_id` is not reserved and valid, but the platform does not support it due to one or more missing dependencies (Hardware or SBI implementation).
/// | `SbiRet::invalid_state()`   | `event_id` is valid but the event is not in `REGISTERED` state.
/// | `SbiRet::invalid_param()`   | `event_id` is invalid.
#[doc(alias = "sbi_sse_unregister")]
#[inline]
pub fn sse_unregister(event_id: u32) -> SbiRet {
    sbi_call_1(EID_SSE, UNREGISTER, event_id as _)
}

/// Enable the software event specified by the `event_id` parameter.
///
/// For local events, the event is enabled only for the calling hart.
/// For global events, the event is enabled for all the harts.
///
/// The event MUST be in `REGISTERED` state otherwise this function will fail.
///
/// Upon success, the event state moves from `REGISTERED` to `ENABLED`. In case of an error, possible error codes are listed below.
///
/// # Return value
///
/// | Error code                  | Description
/// |:----------------------------|:---------------------------------
/// | `SbiRet::success()`         | Event is successfully enabled.
/// | `SbiRet::not_supported()`   | `event_id` is not reserved and valid, but the platform does not support it due to one or more missing dependencies (Hardware or SBI implementation).
/// | `SbiRet::invalid_param()`   | `event_id` is invalid.
/// | `SbiRet::invalid_state()`   | `event_id` is valid but the event is not in `REGISTERED` state.
#[doc(alias = "sbi_sse_enable")]
#[inline]
pub fn sse_enable(event_id: u32) -> SbiRet {
    sbi_call_1(EID_SSE, ENABLE, event_id as _)
}

/// Disable the software event specified by the `event_id` parameter.
///
/// For local events, the event is disabled only for the calling hart.
/// For global events, the event is disabled for all the harts.
///
/// The event MUST be in `ENABLED` state otherwise this function will fail.
///
/// Upon success, the event state moves from `ENABLED` to `REGISTERED`. In case of an error, possible error codes are listed below.
///
/// # Return value
///
/// | Error code                  | Description
/// |:----------------------------|:---------------------------------
/// | `SbiRet::success()`         | Event is successfully disabled.
/// | `SbiRet::not_supported()`   | `event_id` is not reserved and valid, but the platform does not support it due to one or more missing dependencies (Hardware or SBI implementation).
/// | `SbiRet::invalid_param()`   | `event_id` is invalid.
/// | `SbiRet::invalid_state()`   | `event_id` is valid but the event is not in `ENABLED` state.
#[doc(alias = "sbi_sse_disable")]
#[inline]
pub fn sse_disable(event_id: u32) -> SbiRet {
    sbi_call_1(EID_SSE, DISABLE, event_id as _)
}

/// Complete the supervisor event handling for the highest priority event in `RUNNING` state on the calling hart.
///
/// If there were no events in `RUNNING` state on the calling hart then this function does nothing and returns `SBI_SUCCESS`
/// otherwise it moves the highest priority event in `RUNNING` state to:
///
/// - `REGISTERED` if the event is configured as one-shot
/// - `ENABLED` state otherwise
///
/// It then resumes the interrupted supervisor state.
#[doc(alias = "sbi_sse_complete")]
#[inline]
pub fn sse_complete() -> SbiRet {
    sbi_call_0(EID_SSE, COMPLETE)
}

/// The supervisor software can inject a software event with this function.
///
/// The `event_id` parameter refers to the ID of the event to be injected.
///
/// For local events, the `hart_id` parameter refers to the hart on which the event is to be injected.
/// For global events, the `hart_id` parameter is ignored.
///
/// An event can only be injected if it is allowed by the event attribute.
///
/// If an event is injected from within an SSE event handler, if it is ready to be run,
/// it will be handled according to the priority rules
///
/// - If it has a higher priority than the one currently running, then it will be handled immediately, effectively preempting the currently running one.
/// - If it has a lower priority, it will be run after the one that is currently running completes.
///
/// In case of an error, possible error codes are listed below.
///
/// # Return value
///
/// | Error code                  | Description
/// |:----------------------------|:---------------------------------
/// | `SbiRet::success()`         | Event is successfully injected.
/// | `SbiRet::not_supported()`   | `event_id` is not reserved and valid, but the platform does not support it due to one or more missing dependencies (Hardware or SBI implementation).
/// | `SbiRet::invalid_param()`   | `event_id` is invalid or `hart_id` is invalid.
/// | `SbiRet::failed()`          | The injection failed for unspecified or unknown other reasons.
#[doc(alias = "sbi_sse_inject")]
#[inline]
pub fn sse_inject(event_id: u32, hart_id: usize) -> SbiRet {
    sbi_call_2(EID_SSE, INJECT, event_id as _, hart_id)
}

/// Start receiving (or unmask) software events on the calling hart.
/// In other words, the calling hart is ready to receive software events from the SBI implementation.
///
/// The software events are masked initially on all harts so the supervisor software must
/// explicitly unmask software events on relevant harts at boot-time.
///
/// In case of an error, possible error codes are listed below.
///
/// # Return value
///
/// | Error code                  | Description
/// |:----------------------------|:---------------------------------
/// | `SbiRet::success()`         | Software events unmasked successfully on the calling hart.
/// | `SbiRet::already_started()` | Software events were already unmasked on the calling hart.
/// | `SbiRet::failed()`          | The request failed for unspecified or unknown other reasons.
#[doc(alias = "sbi_sse_hart_unmask")]
#[inline]
pub fn sse_hart_unmask() -> SbiRet {
    sbi_call_0(EID_SSE, HART_UNMASK)
}

/// Stop receiving (or mask) software events on the calling hart.
/// In other words, the calling hart will no longer be ready to receive software events from the SBI implementation.
///
/// In case of an error, possible error codes are listed below.
///
/// # Return value
///
/// | Error code                  | Description
/// |:----------------------------|:---------------------------------
/// | `SbiRet::success()`         | Software events masked successfully on the calling hart.
/// | `SbiRet::already_stopped()` | Software events were already masked on the calling hart.
/// | `SbiRet::failed()`          | The request failed for unspecified or unknown other reasons.
#[doc(alias = "sbi_sse_hart_mask")]
#[inline]
pub fn sse_hart_mask() -> SbiRet {
    sbi_call_0(EID_SSE, HART_MASK)
}
