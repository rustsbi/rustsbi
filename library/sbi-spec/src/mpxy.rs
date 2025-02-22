//! Chapter 20. Message Proxy Extension (EID #0x4D505859 "MPXY")

/// Extension ID for Message Proxy Extension.
#[doc(alias = "SBI_EXT_MPXY")]
pub const EID_MPXY: usize = crate::eid_from_str("MPXY") as _;
pub use fid::*;

/// Declared in §20.12.
mod fid {
    /// Function ID to get the version of the message proxy extension.
    ///
    /// Declared in §20.5.
    #[doc(alias = "SBI_EXT_MPXY_GET_SHMEM_SIZE")]
    pub const GET_SHMEM_SIZE: usize = 0;
    /// Function ID to set the shared memory for sending and receiving messages on the calling hart.
    ///
    /// Declared in §20.6.
    #[doc(alias = "SBI_EXT_MPXY_SET_SHMEM")]
    pub const SET_SHMEM: usize = 1;
    /// Function ID to get channel ids of the message channels accessible to the supervisor software in the shared memory of the calling hart.
    ///
    /// Declared in §20.7.
    #[doc(alias = "SBI_EXT_MPXY_GET_CHANNEL_IDS")]
    pub const GET_CHANNEL_IDS: usize = 2;
    /// Function ID to read message channel attributes.
    ///
    /// Declared in §20.8.
    #[doc(alias = "SBI_EXT_MPXY_READ_ATTRS")]
    pub const READ_ATTRIBUTE: usize = 3;
    /// Function ID to write message channel attributes.
    ///
    /// Declared in §20.9.
    #[doc(alias = "SBI_EXT_MPXY_WRITE_ATTRS")]
    pub const WRITE_ATTRIBUTE: usize = 4;
    /// Function ID to send a message to the mpxy channel and waits for sbi implementation for the message response.
    ///
    /// Declared in 20.10.
    #[doc(alias = "SBI_EXT_MPXY_SEND_MSG_WITH_RESP")]
    pub const SEND_MESSAGE_WITH_RESPONSE: usize = 5;
    /// Function ID to send a message to the mpxy channel and does not waits for response.
    ///
    /// Declared in 20.11.
    #[doc(alias = "SBI_EXT_MPXY_SEND_MSG_WITHOUT_RESP")]
    pub const SEND_MESSAGE_WITHOUT_RESPONSE: usize = 6;
    /// Function ID to get the message protocol specific notification events on the mpxy channel.
    ///
    /// Declared in 20.12.
    #[doc(alias = "SBI_EXT_MPXY_GET_NOTIFICATION_EVENTS")]
    pub const GET_NOTIFICATION_EVENTS: usize = 7;
}
