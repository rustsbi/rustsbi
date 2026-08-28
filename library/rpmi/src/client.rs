//! RPMI service group clients built on the shared-memory mailbox.
//!
//! Provides the Base service group client used to probe other service
//! groups and query the platform, mirroring OpenSBI
//! `lib/utils/mailbox/fdt_mailbox_rpmi_shmem.c` (`smq_base_get_two_u32`,
//! `rpmi_get_platform_info`).

use crate::mailbox::RpmiMailbox;
use crate::message::{Error, base_service, servicegroup};

/// Base service group client.
pub struct BaseClient<'a> {
    mailbox: &'a mut RpmiMailbox,
}

impl<'a> BaseClient<'a> {
    /// Create a new Base service group client over a mailbox.
    pub fn new(mailbox: &'a mut RpmiMailbox) -> Self {
        Self { mailbox }
    }

    /// Send a Base service request that takes one u32 input and returns
    /// the single data word of the response (mirrors OpenSBI
    /// `smq_base_get_two_u32`).
    ///
    /// The Base services with one input word (`PROBE_SERVICE_GROUP`,
    /// `GET_SPEC_VERSION`, `GET_IMPLEMENTATION_VERSION`) reply with
    /// `[status(4)][value(4)]`: the value is the first word after the
    /// status, i.e. `resp[4..8]`.
    fn get_two_u32(&mut self, service_id: u8, inarg: u32) -> Result<u32, Error> {
        let mut resp = [0u8; 8];
        let req = inarg.to_le_bytes();
        let (err, _len) = self
            .mailbox
            .normal_request_with_status(servicegroup::BASE, service_id, &req, &mut resp)
            .map_err(|_| Error::Timeout)?;
        if err != Error::Success {
            return Err(err);
        }
        Ok(u32::from_le_bytes([resp[4], resp[5], resp[6], resp[7]]))
    }

    /// Probe a service group and return its version.
    ///
    /// A version of `0` means the group is not implemented (mirrors OpenSBI
    /// `rpmi_shmem_mbox_request_chan`, which rejects a probe returning
    /// version 0); a non-success status is returned as `Err`.
    pub fn probe_service_group(&mut self, group_id: u16) -> Result<u32, Error> {
        self.get_two_u32(base_service::PROBE_SERVICE_GROUP, group_id as u32)
    }

    /// Get the RPMI specification version implemented by the platform.
    pub fn get_spec_version(&mut self) -> Result<u32, Error> {
        self.get_two_u32(base_service::GET_SPEC_VERSION, 0)
    }

    /// Get the implementation version of the platform management
    /// processor firmware.
    pub fn get_implementation_version(&mut self) -> Result<u32, Error> {
        self.get_two_u32(base_service::GET_IMPLEMENTATION_VERSION, 0)
    }

    /// Get the platform information string.
    ///
    /// `buf` receives the platform information; the returned slice is the
    /// portion actually filled.
    ///
    /// The acknowledgement is received through a fixed 256-byte stack
    /// buffer, so platform information longer than 248 bytes is truncated
    /// (this crate has no allocator; OpenSBI instead allocates
    /// `RPMI_MSG_DATA_SIZE(slot_size)` for the response).
    pub fn get_platform_info<'b>(&mut self, buf: &'b mut [u8]) -> Result<&'b [u8], Error> {
        let mut resp = [0u8; 256];
        let (err, len_rx) = self
            .mailbox
            .normal_request_with_status(
                servicegroup::BASE,
                base_service::GET_PLATFORM_INFO,
                &[],
                &mut resp,
            )
            .map_err(|_| Error::Timeout)?;
        if err != Error::Success {
            return Err(err);
        }
        let len = u32::from_le_bytes([resp[4], resp[5], resp[6], resp[7]]) as usize;
        // `len` (PLATFORM_ID_LEN) is written by the PuC and must not be
        // trusted: clamp the copy to the bytes actually received (`len_rx`
        // includes the status and length words) and the caller's buffer.
        let avail = len_rx.saturating_sub(8);
        let n = len.min(avail).min(buf.len());
        buf[..n].copy_from_slice(&resp[8..8 + n]);
        Ok(&buf[..n])
    }

    /// Enable notification for the given event on the Base service group.
    ///
    /// Mirrors `rpmi_enable_notification_req` / `resp`; the request carries
    /// `[event_id(4)][req_state(4)]` with `req_state` set to `ENABLE` (1).
    pub fn enable_notification(&mut self, event_id: u32) -> Result<(), Error> {
        let mut resp = [0u8; 4];
        // Request = [event_id(4)][req_state(4)]; req_state 1 = enable
        // (RPMI_EVENT_NOTIF_ENABLE_STATE).
        let mut req = [0u8; 8];
        req[..4].copy_from_slice(&event_id.to_le_bytes());
        req[4..].copy_from_slice(&1u32.to_le_bytes());
        let (err, _len) = self
            .mailbox
            .normal_request_with_status(
                servicegroup::BASE,
                base_service::ENABLE_NOTIFICATION,
                &req,
                &mut resp,
            )
            .map_err(|_| Error::Timeout)?;
        if err != Error::Success {
            return Err(err);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    extern crate std;
    use std::boxed::Box;

    use super::*;
    use crate::message::{MessageHeader, MessageType};
    use crate::smq::{Le32, SmqQueue};

    /// Build a mailbox with dedicated A2P_REQ / P2A_ACK rings over leaked
    /// heap buffers, returning the mailbox plus a PuC-side view of the
    /// P2A_ACK ring used to pre-load acknowledgements.
    fn test_mailbox() -> (RpmiMailbox, SmqQueue) {
        let a2p_buffer = Box::leak(Box::new([0u8; 4 * 64]));
        let a2p_indices = Box::leak(Box::new([Le32(0); 2]));
        let p2a_buffer = Box::leak(Box::new([0u8; 4 * 64]));
        let p2a_indices = Box::leak(Box::new([Le32(0); 2]));
        let dummy_buffer = Box::leak(Box::new([0u8; 4 * 64]));
        let dummy_indices = Box::leak(Box::new([Le32(0); 2]));

        let q_a2p = unsafe {
            SmqQueue::new(
                a2p_indices.as_ptr(),
                a2p_indices.as_ptr().add(1),
                a2p_buffer.as_mut_ptr(),
                64,
                4,
            )
        };
        let q_p2a = unsafe {
            SmqQueue::new(
                p2a_indices.as_ptr(),
                p2a_indices.as_ptr().add(1),
                p2a_buffer.as_mut_ptr(),
                64,
                4,
            )
        };
        let q_p2a_puc = unsafe {
            SmqQueue::new(
                p2a_indices.as_ptr(),
                p2a_indices.as_ptr().add(1),
                p2a_buffer.as_mut_ptr(),
                64,
                4,
            )
        };
        let mut q_dummy = || unsafe {
            SmqQueue::new(
                dummy_indices.as_ptr(),
                dummy_indices.as_ptr().add(1),
                dummy_buffer.as_mut_ptr(),
                64,
                4,
            )
        };
        let mailbox = unsafe { RpmiMailbox::new(64, [q_a2p, q_p2a, q_dummy(), q_dummy()], None) };
        (mailbox, q_p2a_puc)
    }

    #[test]
    fn test_get_platform_info_clamps_oversized_length() {
        let (mut mailbox, q_p2a_puc) = test_mailbox();

        // The first request issued by the mailbox always carries token 2
        // (the counter starts at 1 and increments before returning).
        // Pre-load the matching acknowledgement with a PLATFORM_ID_LEN (300)
        // that exceeds the fixed 256-byte response buffer: the client must
        // clamp the copy to the bytes actually received instead of slicing
        // out of bounds.
        let header = MessageHeader::new(
            servicegroup::BASE,
            base_service::GET_PLATFORM_INFO,
            MessageType::Acknowledgement,
            56,
            2,
        );
        let mut payload = [0u8; 56];
        payload[0..4].copy_from_slice(&0u32.to_le_bytes()); // status: success
        payload[4..8].copy_from_slice(&300u32.to_le_bytes()); // PLATFORM_ID_LEN: oversized
        payload[8..].fill(0xab); // 48 bytes of platform info
        unsafe { q_p2a_puc.send(&header, &payload, None) }.unwrap();

        let mut client = BaseClient::new(&mut mailbox);
        let mut buf = [0u8; 512];
        let info = client.get_platform_info(&mut buf).expect("platform info");
        // Only the 48 info bytes actually received are returned, not the
        // claimed (oversized) 300.
        assert_eq!(info.len(), 48);
        assert!(info.iter().all(|&b| b == 0xab));
    }
}
