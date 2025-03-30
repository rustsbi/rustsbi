//! Chapter 20. Message Proxy Extension (EID #0x4D505859 “MPXY”)

use crate::binary::{sbi_call_0, sbi_call_1, sbi_call_3};
use sbi_spec::{
    binary::{SbiRet, SharedPtr},
    mpxy::*,
};

/// Get the shared memory size in number of bytes for sending and receiving messages.
///
/// The shared memory size returned by the SBI implementation MUST satisfy the following requirements:
/// - The shared memory size MUST be same for all HARTs.
/// - The shared memory size MUST be at least 4096 bytes.
/// - The shared memory size MUST be multiple of 4096 bytes.
/// - The shared memory size MUST not be less than the biggest MSG_DATA_MAX_LEN attribute value across all MPXY channels.
///
/// # Return value
///
/// This function always returns `SbiRet::success()` in `SbiRet.error` and it will return the shared memory size.
#[doc(alias = "sbi_mpxy_get_shmem_size")]
#[inline]
pub fn mpxy_get_shmem_size() -> usize {
    sbi_call_0(EID_MPXY, GET_SHMEM_SIZE).value
}

/// Set the shared memory for sending and receiving messages on the calling hart.
///
/// # Parameters
///
/// - `shmem`: shared memory pointer.
///
/// If `shmem` is not all-ones bitwise then `shmem` specifies the bits of the shared memory physical base address.
/// The `shmem` MUST be `4096` bytes aligned and the size of shared memory is assumed to be same as returned by the Get shared memory size function: mpxy_get_shmem_size().
/// If `shmem` is all-ones bitwise then shared memory is disabled.
///
/// - `flags`: the parameter specifies configuration for shared memory setup and it is encoded as follows:
///
/// ```text
/// flags[XLEN-1:2]: Reserved for future use and must be zero.
/// flags[1:0]: Shared memory setup mode (Refer table below).
/// ```
///
/// | Mode             | flags[1:0]  | Description
/// |:-----------------|:------------|:---------------------------------
/// | OVERWRITE        | 0b00        | Ignore the current shared memory state and force setup the new shared memory based on the passed parameters.
/// | OVERWRITE-RETURN | 0b01        | Same as `OVERWRITE` mode and additionally after the new shared memory state is enabled, the old shared memory are written in the same order to the new shared memory at offset `0x0`. This flag provide provision to software layers in the supervisor software that want to send messages using the shared memory but do not know the shared memory details that has already been setup. Those software layers can temporarily setup their own shared memory on the calling hart, send messages and then restore back the previous shared memory with the SBI implementation.
/// | RESERVED         | 0b10 - 0b11 | Reserved for future use. Must be initialized to `0`.
///
/// # Return value
///
/// | Error code                    | Description
/// |:------------------------------|:---------------------------------
/// | `SbiRet::success()`           | Shared memory was set or cleared successfully.
/// | `SbiRet::invalid_param()`     | The `flags` parameter has invalid value or the bits set are within the reserved range. Or the `shmem` parameter is not `4096` bytes aligned.
/// | `SbiRet::invalid_address()`   | The shared memory pointed to by the `shmem` parameter does not satisfy the requirements.
/// | `SbiRet::failed()`            | The request failed for unspecified or unknown other reasons.
#[doc(alias = "sbi_mpxy_set_shmem")]
#[inline]
pub fn mpxy_set_shmem(shmem: SharedPtr<u8>, flags: usize) -> SbiRet {
    sbi_call_3(
        EID_MPXY,
        SET_SHMEM,
        shmem.phys_addr_lo(),
        shmem.phys_addr_hi(),
        flags,
    )
}

/// Get channel IDs of the message channels accessible to the supervisor software in the shared memory of the calling hart.
///
/// The channel IDs are returned as an array of 32 bits unsigned integers where the `start_index` parameter specifies the array index
/// of the first channel ID to be returned in the shared memory.
///
/// The SBI implementation will return channel IDs in the shared memory of the calling hart as specified by the table below:
///
/// | Offset            | Field                            | Description
/// |:------------------|:---------------------------------|:---------------------------------
/// | 0x0               | REMAINING                        | Remaining number of channel IDs.
/// | 0x4               | RETURNED                         | Number of channel IDs (N) returned in the shared memory.
/// | 0x8               | CHANNEL_ID [start_index + 0]     | Channel ID
/// | 0xC               | CHANNEL_ID [start_index + 1]     | Channel ID
/// | 0x8 + ((N-1) * 4) | CHANNEL_ID [start_index + N - 1] | Channel ID
///
/// The number of channel IDs returned in the shared memory are specified by the `RETURNED` field whereas the `REMAINING` field specifies the
/// number of remaining channel IDs. If the `REMAINING` is not `0` then supervisor software can call this function again to get remaining channel
/// IDs with `start_index` passed accordingly. The supervisor software may require multiple SBI calls to get the complete list of channel IDs
/// depending on the `RETURNED` and `REMAINING` fields.
///
/// # Parameters
///
/// - `start_index`: specifies the array index of the first channel ID to be returned in the shared memory.
///
/// # Return value
///
/// The `SbiRet.value` is always set to zero whereas the possible error codes returned in `SbiRet.error` are below.
///
/// | Error code                    | Description
/// |:------------------------------|:---------------------------------
/// | `SbiRet::success()`           | The channel ID array has been written successfully.
/// | `SbiRet::invalid_param()`     | `start_index` is invalid.
/// | `SbiRet::no_shmem()`          | The shared memory setup is not done or disabled for the calling hart.
/// | `SbiRet::denied()`            | Getting channel ID array is not allowed on the calling hart.
/// | `SbiRet::failed()`            | The request failed for unspecified or unknown other reasons.
#[doc(alias = "sbi_mpxy_get_channel_ids")]
#[inline]
pub fn mpxy_get_channel_ids(start_index: u32) -> SbiRet {
    sbi_call_1(EID_MPXY, GET_CHANNEL_IDS, start_index as usize)
}

/// Read message channel attributes.
///
/// Supervisor software MUST call this function for the contiguous attribute range where the `base_attribute_id` is the starting index of that
/// range and `attribute_count` is the number of attributes in the contiguous range. If there are multiple such attribute ranges then multiple
/// calls of this function may be done from supervisor software. Supervisor software MUST read the message protocol specific attributes via
/// separate call to this function with `base_attribute_id` and `attribute_count` without any overlap with the MPXY standard attributes.
///
/// # Parameters
///
/// - `channel_id`: specifies the message channel whereas `base_attribute_id` and `attribute_count` parameters specify the range of attribute ids to be read.
/// - `base_attribute_id`: specifies the range of attribute ids to be read.
/// - `attribute_count`: specifies the range of attribute ids to be read.
///
/// # Return value
///
/// Upon calling this function the message channel attribute values are returned starting from the offset `0x0` in the shared memory of the
/// calling hart where the value of the attribute with `attribute_id = base_attribute_id + i` is available at the shared memory offset `4 * i`.
///
/// The possible error codes returned in `SbiRet.error` are shown below.
///
/// | Error code                    | Description
/// |:------------------------------|:---------------------------------
/// | `SbiRet::success()`           | Message channel attributes has been read successfully.
/// | `SbiRet::invalid_param()`     | `attribute_count` is 0. Or the `attribute_count > (shared memory size)/4`. Or the `base_attribute_id` is not valid.
/// | `SbiRet::not_supported()`     | `channel_id` is not supported or invalid.
/// | `SbiRet::bad_range()`         | One of the attributes in the range specified by the `base_attribute_id` and `attribute_count` do not exist.
/// | `SbiRet::no_shmem()`          | The shared memory setup is not done or disabled for calling hart.
/// | `SbiRet::failed()`            | The request failed for unspecified or unknown other reasons.
#[doc(alias = "sbi_mpxy_read_attributes")]
#[inline]
pub fn mpxy_read_attributes(
    channel_id: u32,
    base_attribute_id: u32,
    attribute_count: u32,
) -> SbiRet {
    sbi_call_3(
        EID_MPXY,
        READ_ATTRIBUTE,
        channel_id as usize,
        base_attribute_id as usize,
        attribute_count as usize,
    )
}

/// Write message channel attributes.
///
/// Supervisor software MUST call this function for the contiguous attribute range where the `base_attribute_id` is the starting index of that
/// range and `attribute_count` is the number of attributes in the contiguous range. If there are multiple such attribute ranges then multiple
/// calls of this function may be done from supervisor software. Apart from contiguous attribute indices, supervisor software MUST also
/// consider the attribute access permissions and attributes with RO (Read Only) access MUST be excluded from the attribute range.
/// Supervisor software MUST write the message protocol specific attributes via separate call to this function with `base_attribute_id` and
/// `attribute_count` without any overlap with the MPXY standard attributes.
///
/// Before calling this function, the supervisor software must populate the shared memory of the calling hart starting from offset `0x0` with the
/// message channel attribute values. For each attribute with `attribute_id = base_attribute_id + i`, the corresponding value MUST be placed at
/// the shared memory offset `4 * i`.
///
/// # Parameters
///
/// - `channel_id`: specifies the message channel whereas `base_attribute_id` and `attribute_count` parameters specify the range of attribute ids.
/// - `base_attribute_id`: specifies the range of attribute ids.
/// - `attribute_count`: specifies the range of attribute ids.
///
/// # Return value
///
/// The possible error codes returned in `SbiRet.error` are shown below.
///
/// | Error code                    | Description
/// |:------------------------------|:---------------------------------
/// | `SbiRet::success()`           | Message channel attributes has been written successfully.
/// | `SbiRet::invalid_param()`     | `attribute_count` is 0. Or the `attribute_count > (shared memory size)/4`. Or the `base_attribute_id` is not valid.
/// | `SbiRet::not_supported()`     | `channel_id` is not supported or invalid.
/// | `SbiRet::bad_range()`         | One of the attributes in the range specified by the `base_attribute_id` and `attribute_count` do not exist or the attribute is read-only (RO). Or `base_attribute_id` and `attribute_count` result into a range which overlaps with standard and message protocol specific attributes.
/// | `SbiRet::no_shmem()`          | The shared memory setup is not done or disabled for calling hart.
/// | `SbiRet::denied()`            | If any attribute write dependency is not satisfied.
/// | `SbiRet::failed()`            | The request failed for unspecified or unknown other reasons.
#[doc(alias = "sbi_mpxy_write_attributes")]
#[inline]
pub fn mpxy_write_attributes(
    channel_id: u32,
    base_attribute_id: u32,
    attribute_count: u32,
) -> SbiRet {
    sbi_call_3(
        EID_MPXY,
        WRITE_ATTRIBUTE,
        channel_id as usize,
        base_attribute_id as usize,
        attribute_count as usize,
    )
}

/// Send a message to the MPXY channel specified by the `channel_id` parameter and wait until a message response is received from the MPXY channel.
///
/// This function only succeeds upon receipt of a message response from the MPXY channel. In cases where complete data transfer requires
/// multiple transmissions, the supervisor software shall send multiple messages as necessary. Details of such cases can be found in
/// respective message protocol specifications.
///
/// This function is optional. If this function is implemented, the corresponding bit in the `CHANNEL_CAPABILITY` attribute is set to `1`.
///
/// # Parameters
///
/// - `channel_id`: specifies the MPXY channel.
/// - `message_id`: specifies the message protocol specific identification of the message to be sent.
/// - `message_data_len`: represents the length of message data in bytes which is located at the offset `0x0` in the shared memory setup by the calling hart.
///
/// # Return value
///
/// Upon calling this function the SBI implementation MUST write the response message data at the offset `0x0` in the shared memory setup by
/// the calling hart and the number of bytes written will be returned through `SbiRet.value`. The layout of data in case of both request and
/// response is according to the respective message protocol specification message format.
///
/// Upon success, this function:
/// - Writes the message response data at offset `0x0` of the shared memory setup by the calling hart.
/// - Returns `SbiRet::success()` in `SbiRet.error`.
/// - Returns message response data length in `SbiRet.value`.
///
/// The possible error codes returned in `SbiRet.error` are shown below.
///
/// | Error code                    | Description
/// |:------------------------------|:---------------------------------
/// | `SbiRet::success()`           | Message sent and response received successfully.
/// | `SbiRet::invalid_param()`     | The `message_data_len > MSG_DATA_MAX_LEN` for specified `channel_id`. Or the `message_data_len` is greater than the size of shared memory on the calling hart.
/// | `SbiRet::not_supported()`     | `channel_id` is not supported or invalid. Or the message represented by the `message_id` is not supported or invalid. Or this function is not supported.
/// | `SbiRet::no_shmem()`          | The shared memory setup is not done or disabled for calling hart.
/// | `SbiRet::timeout()`           | Waiting for response timeout.
/// | `SbiRet::io()`                | Failed due to I/O error.
/// | `SbiRet::failed()`            | The request failed for unspecified or unknown other reasons.
#[doc(alias = "sbi_mpxy_send_message_with_response")]
#[inline]
pub fn mpxy_send_message_with_response(
    channel_id: u32,
    message_id: u32,
    message_data_len: usize,
) -> SbiRet {
    sbi_call_3(
        EID_MPXY,
        SEND_MESSAGE_WITH_RESPONSE,
        channel_id as usize,
        message_id as usize,
        message_data_len,
    )
}

/// Send a message to the MPXY channel specified by the `channel_id` parameter without waiting for a message response from the MPXY channel.
///
/// This function does not wait for message response from the channel and returns after successful message transmission. In cases where
/// complete data transfer requires multiple transmissions, the supervisor software shall send multiple messages as necessary. Details of such
/// cases can be found in the respective message protocol specification.
///
/// This function is optional. If this function is implemented, the corresponding bit in the `CHANNEL_CAPABILITY` attribute is set to `1`.
///
/// # Parameters
///
/// - `channel_id`: specifies the MPXY channel.
/// - `message_id`: specifies the message protocol specific identification of the message to be sent.
/// - `message_data_len`: represents the length of message data in bytes which is located at the offset `0x0` in the shared memory setup by the calling hart.
///
/// # Return value
///
/// The possible error codes returned in `SbiRet.error` are shown below.
///
/// | Error code                    | Description
/// |:------------------------------|:---------------------------------
/// | `SbiRet::success()`           | Message sent successfully.
/// | `SbiRet::invalid_param()`     | The `message_data_len > MSG_DATA_MAX_LEN` for specified `channel_id`. Or the `message_data_len` is greater than the size of shared memory on the calling hart.
/// | `SbiRet::not_supported()`     | `channel_id` is not supported or invalid. Or the message represented by the `message_id` is not supported or invalid. Or this function is not supported.
/// | `SbiRet::no_shmem()`          | The shared memory setup is not done or disabled for calling hart.
/// | `SbiRet::timeout()`           | Message send timeout.
/// | `SbiRet::io()`                | Failed due to I/O error.
/// | `SbiRet::failed()`            | The request failed for unspecified or unknown other reasons.
#[doc(alias = "sbi_mpxy_send_message_without_response")]
#[inline]
pub fn mpxy_send_message_without_response(
    channel_id: u32,
    message_id: u32,
    message_data_len: usize,
) -> SbiRet {
    sbi_call_3(
        EID_MPXY,
        SEND_MESSAGE_WITHOUT_RESPONSE,
        channel_id as usize,
        message_id as usize,
        message_data_len,
    )
}

/// Get the message protocol specific notification events on the MPXY channel specified by the `channel_id` parameter.
///
/// The events are message protocol specific and MUST be defined in the respective message protocol specification.
/// The SBI implementation may support indication mechanisms like MSI or SSE to inform the supervisor software about the availability of events.
///
/// Depending on the message protocol implementation, a channel may support events state which includes data like number of events
/// `RETURNED`, `REMAINING` and `LOST`. Events state data is optional, and if the message protocol implementation supports it, then the channel
/// will have the corresponding bit set in the `CHANNEL_CAPABILITY` attribute. By default the events state is disabled and supervisor software
/// can explicitly enable it through the `EVENTS_STATE_CONTROL` attribute.
///
/// This function is optional. If this function is implemented, the corresponding bit in the `CHANNEL_CAPABILITY` attribute is set to 1.
///
/// # Parameters
///
/// - `channel_id`: specifies the MPXY channel.
///
/// # Return value
///
/// In the shared memory, 16 bytes starting from offset 0x0 are used to return this state data.
/// Shared memory layout with events state data (each field is of 4 bytes):
///
/// ```text
/// Offset 0x0: REMAINING
/// Offset 0x4: RETURNED
/// Offset 0x8: LOST
/// Offset 0xC: RESERVED
/// Offset 0x10: Start of message protocol specific notification events data
/// ```
///
/// The `RETURNED` field represents the number of events which are returned in the shared memory when this function is called. The `REMAINING`
/// field represents the number of events still remaining with SBI implementation. The supervisor software may need to call this function again
/// until the `REMAINING` field becomes `0`.
///
/// The `LOST` field represents the number of events which are lost due to limited buffer size managed by the message protocol
/// implementation. Details of buffering/caching of events is specific to message protocol implementation.
///
/// Upon calling this function the received notification events are written by the SBI implementation at the offset `0x10` in the shared memory
/// setup by the calling hart irrespective of events state data reporting. If events state data reporting is disabled or not supported, then the
/// values in events state fields are undefined. The number of the bytes written to the shared memory will be returned through `SbiRet.value`
/// which is the number of bytes starting from offset `0x10`. The layout and encoding of notification events are defined by the message
/// protocol specification associated with the message proxy channel (`channel_id`).
///
/// The possible error codes returned in `SbiRet.error` are shown below.
///
/// | Error code                    | Description
/// |:------------------------------|:---------------------------------
/// | `SbiRet::success()`           | Notifications received successfully.
/// | `SbiRet::not_supported()`     | `channel_id` is not supported or invalid. Or this function is not supported.
/// | `SbiRet::no_shmem()`          | The shared memory setup is not done or disabled for calling hart.
/// | `SbiRet::io()`                | Failed due to I/O error.
/// | `SbiRet::failed()`            | The request failed for unspecified or unknown other reasons.
#[doc(alias = "sbi_mpxy_get_notification_events")]
#[inline]
pub fn mpxy_get_notification_events(channel_id: u32) -> SbiRet {
    sbi_call_1(EID_MPXY, GET_NOTIFICATION_EVENTS, channel_id as usize)
}
