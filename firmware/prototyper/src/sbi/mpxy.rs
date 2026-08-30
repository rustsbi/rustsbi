use alloc::vec::Vec;
use core::sync::atomic::{AtomicUsize, Ordering};

use rustsbi::SbiRet;
use sbi_spec::binary::SharedPtr;

use crate::rpmi::servicegroup;
use crate::rpmi::{MailboxError, RpmiMailbox};

// Standard MPXY channel attribute identifiers from the SBI specification.
mod channel_attr {
    pub const MSG_PROT_ID: u32 = 0;
    pub const MSG_PROT_VERSION: u32 = 1;
    pub const MSG_MAX_LEN: u32 = 2;
    pub const MSG_SEND_TIMEOUT: u32 = 3;
    pub const MSG_COMPLETION_TIMEOUT: u32 = 4;
    pub const CAPABILITY: u32 = 5;
    pub const SSE_EVENT_ID: u32 = 6;
    pub const MSI_CONTROL: u32 = 7;
    pub const MSI_ADDR_LO: u32 = 8;
    pub const MSI_ADDR_HI: u32 = 9;
    pub const MSI_DATA: u32 = 10;
    pub const EVENTS_STATE_CONTROL: u32 = 11;
    // RPMI message-protocol attributes start at `0x8000_0000`.
    pub const RPMI_SERVICEGROUP_ID: u32 = 0x8000_0000;
    pub const RPMI_SERVICEGROUP_VERSION: u32 = 0x8000_0001;
    pub const RPMI_IMPL_ID: u32 = 0x8000_0002;
    pub const RPMI_IMPL_VERSION: u32 = 0x8000_0003;
}

// MPXY assigns protocol ID zero to RPMI.
const RPMI_MSGPROTO_ID: u32 = 0x0;
const CHANNEL_CAPABILITY: u32 = 1 << 3;

// SBI v3.0 excludes RPMI groups already exposed through a dedicated SBI
// extension.
const CHANNEL_COMPATIBLES: &[(&str, u16)] = &[
    ("riscv,rpmi-mpxy-voltage", servicegroup::VOLTAGE),
    ("riscv,rpmi-mpxy-clock", servicegroup::CLOCK),
    ("riscv,rpmi-mpxy-domain", servicegroup::DOMAIN),
];

#[derive(Clone, Copy)]
struct Channel {
    channel_id: u32,
    service_group_id: u16,
    version: u32,
}

/// Message Proxy extension backed by the RPMI mailbox.
///
/// Each channel is bound to one RPMI service group, and message IDs map to
/// service IDs within that group. Operations require a platform-provided
/// mailbox backend.
pub(crate) struct SbiMpxy {
    mailbox: &'static RpmiMailbox,
    protocol_info: crate::rpmi::RpmiProtocolInfo,
    channels: Vec<Channel>,
    // MPXY shared memory is configured independently for each hart.
    shmem: [AtomicUsize; crate::cfg::NUM_HART_MAX],
}

impl SbiMpxy {
    /// Create an MPXY extension for RPMI channels described by the device
    /// tree.
    pub(crate) fn from_fdt(
        root: &serde_device_tree::buildin::Node,
        mailbox: &'static RpmiMailbox,
    ) -> Option<Self> {
        let protocol_info = mailbox.protocol_info()?;
        if protocol_info.spec_version >> 16 != 1 {
            return None;
        }
        let mut channel_descriptions = Vec::new();
        crate::devicetree::search_with_parent(root, &mut |node, _| {
            let Some(compatible) = node.get_prop("compatible") else {
                return;
            };
            let compatible = compatible.deserialize::<serde_device_tree::buildin::StrSeq>();
            let Some(service_group_id) = compatible.iter().find_map(|value| {
                CHANNEL_COMPATIBLES
                    .iter()
                    .find(|(name, _)| *name == value)
                    .map(|(_, id)| *id)
            }) else {
                return;
            };
            let Some(channel_id) = node
                .get_prop("riscv,sbi-mpxy-channel-id")
                .map(|value| value.deserialize::<u32>())
            else {
                return;
            };
            if !channel_descriptions
                .iter()
                .any(|(existing_id, _)| *existing_id == channel_id)
            {
                channel_descriptions.push((channel_id, service_group_id));
            }
        });
        let channels = channel_descriptions
            .into_iter()
            .filter_map(|(channel_id, service_group_id)| {
                mailbox
                    .probe_service_group(service_group_id)
                    .filter(|version| version >> 16 == 1)
                    .map(|version| Channel {
                        channel_id,
                        service_group_id,
                        version,
                    })
            })
            .collect::<Vec<_>>();
        if channels.is_empty() {
            return None;
        }
        Some(Self {
            mailbox,
            protocol_info,
            channels,
            shmem: [const { AtomicUsize::new(0) }; crate::cfg::NUM_HART_MAX],
        })
    }

    // Return the current hart's shared-memory slot.
    #[inline]
    fn shmem_hart(&self) -> &AtomicUsize {
        &self.shmem[crate::riscv::current_hartid()]
    }

    fn channel(&self, channel_id: u32) -> Option<&Channel> {
        self.channels
            .iter()
            .find(|channel| channel.channel_id == channel_id)
    }

    fn is_message(service_group_id: u16, message_id: u32) -> bool {
        match service_group_id {
            servicegroup::VOLTAGE | servicegroup::CLOCK => matches!(message_id, 1..=8),
            servicegroup::DOMAIN => matches!(message_id, 1..=5),
            _ => false,
        }
    }

    fn read_channel_attrs(&self, channel_id: u32, base: u32, count: u32, out: &mut [u8]) -> bool {
        let Some(channel) = self
            .channels
            .iter()
            .find(|channel| channel.channel_id == channel_id)
        else {
            return false;
        };
        for i in 0..count {
            let Some(attr) = base.checked_add(i) else {
                return false;
            };
            let value = match attr {
                // Standard SBI MPXY channel attributes
                channel_attr::MSG_PROT_ID => RPMI_MSGPROTO_ID,
                channel_attr::MSG_PROT_VERSION => self.protocol_info.spec_version,
                channel_attr::MSG_MAX_LEN => self.mailbox.message_data_max_len() as u32,
                channel_attr::MSG_SEND_TIMEOUT => crate::rpmi::RPMI_DEF_TX_TIMEOUT * 1_000,
                channel_attr::MSG_COMPLETION_TIMEOUT => {
                    (crate::rpmi::RPMI_DEF_TX_TIMEOUT + crate::rpmi::RPMI_DEF_RX_TIMEOUT) * 1_000
                }
                channel_attr::CAPABILITY => CHANNEL_CAPABILITY,
                channel_attr::SSE_EVENT_ID => 0,
                channel_attr::MSI_CONTROL => 0,
                channel_attr::MSI_ADDR_LO => 0,
                channel_attr::MSI_ADDR_HI => 0,
                channel_attr::MSI_DATA => 0,
                channel_attr::EVENTS_STATE_CONTROL => 0,
                // RPMI message-protocol attributes
                channel_attr::RPMI_SERVICEGROUP_ID => channel.service_group_id as u32,
                channel_attr::RPMI_SERVICEGROUP_VERSION => channel.version,
                channel_attr::RPMI_IMPL_ID => self.protocol_info.implementation_id,
                channel_attr::RPMI_IMPL_VERSION => self.protocol_info.implementation_version,
                _ => return false,
            };
            let off = (i * 4) as usize;
            if off + 4 > out.len() {
                return false;
            }
            out[off..off + 4].copy_from_slice(&value.to_le_bytes());
        }
        true
    }
}

impl rustsbi::Mpxy for SbiMpxy {
    fn get_shmem_size(&self) -> usize {
        // Shared memory for request/response data plus the channel-ID
        // array header; 4 KiB aligned.
        4096
    }

    fn set_shmem(&self, shmem: SharedPtr<u8>, flags: usize) -> SbiRet {
        if flags > 1 {
            return SbiRet::invalid_param();
        }
        let lo = shmem.phys_addr_lo();
        let hi = shmem.phys_addr_hi();
        let all_ones = lo == usize::MAX && hi == usize::MAX;
        if all_ones {
            self.shmem_hart().store(0, Ordering::Release);
            return SbiRet::success(0);
        }
        if lo & 0xfff != 0 {
            return SbiRet::invalid_param();
        }
        if hi != 0 {
            return SbiRet::invalid_address();
        }
        if !crate::firmware::supervisor_writable(lo, self.get_shmem_size()) {
            return SbiRet::invalid_address();
        }
        let previous = self.shmem_hart().swap(lo, Ordering::AcqRel);
        if flags == 1 {
            let (previous_lo, previous_hi) = if previous == 0 {
                (usize::MAX, usize::MAX)
            } else {
                (previous, 0)
            };
            // SAFETY: `lo` identifies the writable 4 KiB region validated
            // above, which is large enough for both XLEN-bit address words.
            unsafe {
                (lo as *mut usize).write_volatile(previous_lo.to_le());
                (lo as *mut usize)
                    .add(1)
                    .write_volatile(previous_hi.to_le());
            }
        }
        SbiRet::success(0)
    }

    fn get_channel_ids(&self, start_index: u32) -> SbiRet {
        let shmem = self.shmem_hart().load(Ordering::Acquire);
        if shmem == 0 {
            return SbiRet::no_shmem();
        }
        // `start_index == count` is valid and returns an empty page. The first
        // two words contain the remaining and returned channel counts.
        let count = self.channels.len() as u32;
        if start_index > count {
            return SbiRet::invalid_param();
        }
        // Number of channel IDs that fit after the remaining/returned fields.
        let max_channelids = (self.get_shmem_size() / 4) - 2;
        let remaining_before = count - start_index;
        let returned = remaining_before.min(max_channelids as u32);
        let remaining_after = count - (start_index + returned);
        // SAFETY: `set_shmem` validated the writable 4 KiB region, and the
        // channel IDs fit within that region.
        let base = shmem as *mut u8;
        unsafe {
            base.add(0).cast::<u32>().write_volatile(remaining_after);
            base.add(4).cast::<u32>().write_volatile(returned);
            for i in 0..returned {
                base.add(8 + i as usize * 4)
                    .cast::<u32>()
                    .write_volatile(self.channels[(start_index + i) as usize].channel_id);
            }
        }
        SbiRet::success(0)
    }

    fn read_attributes(
        &self,
        channel_id: u32,
        base_attribute_id: u32,
        attribute_count: u32,
        _output: SharedPtr<u8>,
    ) -> SbiRet {
        if self.channel(channel_id).is_none() {
            return SbiRet::not_supported();
        }
        if attribute_count == 0 {
            return SbiRet::invalid_param();
        }
        if attribute_count as usize > self.get_shmem_size() / 4 {
            return SbiRet::invalid_param();
        }
        if !matches!(base_attribute_id, 0..=11 | 0x8000_0000..=0x8000_0003) {
            return SbiRet::invalid_param();
        }
        // READ_ATTRIBUTES returns values through the buffer established by
        // SET_SHMEM; the output register argument is not used by the ABI.
        let shmem = self.shmem_hart().load(Ordering::Acquire);
        if shmem == 0 {
            return SbiRet::no_shmem();
        }
        // SAFETY: `set_shmem` validated the writable 4 KiB region, and the
        // attribute count limits this slice to that region.
        let out = unsafe {
            core::slice::from_raw_parts_mut(shmem as *mut u8, (attribute_count as usize) * 4)
        };
        if !self.read_channel_attrs(channel_id, base_attribute_id, attribute_count, out) {
            return SbiRet::bad_range();
        }
        SbiRet::success(0)
    }

    fn write_attributes(
        &self,
        channel_id: u32,
        base_attribute_id: u32,
        attribute_count: u32,
        _input: SharedPtr<u8>,
    ) -> SbiRet {
        if self.channel(channel_id).is_none() {
            return SbiRet::not_supported();
        }
        if attribute_count == 0 {
            return SbiRet::invalid_param();
        }
        if attribute_count as usize > self.get_shmem_size() / 4 {
            return SbiRet::invalid_param();
        }
        if !matches!(base_attribute_id, 0..=11 | 0x8000_0000..=0x8000_0003) {
            return SbiRet::invalid_param();
        }
        let Some(last_attribute_id) = base_attribute_id.checked_add(attribute_count - 1) else {
            return SbiRet::bad_range();
        };
        if base_attribute_id < channel_attr::MSI_CONTROL
            || last_attribute_id > channel_attr::EVENTS_STATE_CONTROL
        {
            return SbiRet::bad_range();
        }
        if self.shmem_hart().load(Ordering::Acquire) == 0 {
            return SbiRet::no_shmem();
        }
        // MSI and event-state reporting are not advertised, so writes to
        // their standard control attributes are ignored.
        SbiRet::success(0)
    }

    fn send_message_with_response(
        &self,
        channel_id: u32,
        message_id: u32,
        message_data_len: usize,
    ) -> SbiRet {
        let Some(channel) = self.channel(channel_id) else {
            return SbiRet::not_supported();
        };
        if !Self::is_message(channel.service_group_id, message_id) {
            return SbiRet::not_supported();
        }
        if message_data_len > self.mailbox.message_data_max_len() {
            return SbiRet::invalid_param();
        }
        if message_data_len % 4 != 0 {
            return SbiRet::invalid_param();
        }
        let shmem = self.shmem_hart().load(Ordering::Acquire);
        if shmem == 0 {
            return SbiRet::no_shmem();
        }
        // MPXY message_id == RPMI service_id.
        let service_id = message_id as u8;
        // SAFETY: `set_shmem` validated the 4 KiB region, and
        // `message_data_len` is bounded by the mailbox slot size.
        let req = unsafe { core::slice::from_raw_parts(shmem as *const u8, message_data_len) };
        let mut resp = alloc::vec![0u8; self.mailbox.message_data_max_len()];
        match self.mailbox.normal_request_with_status(
            channel.service_group_id,
            service_id,
            req,
            &mut resp,
        ) {
            // The S-mode client interprets the RPMI status in the response;
            // translating it into an SBI error would turn a protocol status
            // such as RPMI_ERR_ALREADY into a transport failure.
            Ok((_, len)) => {
                // SAFETY: `resp` contains `len` initialized bytes, and the
                // validated 4 KiB shared buffer is large enough for the reply.
                unsafe {
                    core::ptr::copy_nonoverlapping(resp.as_ptr(), shmem as *mut u8, len);
                }
                SbiRet::success(len)
            }
            // No acknowledgement received: the transfer itself failed.
            Err(MailboxError::Timeout) => {
                warn!(
                    "MPXY send: channel {} service {} timed out (no ack)",
                    channel_id, service_id
                );
                SbiRet::timeout()
            }
            Err(MailboxError::Io) => SbiRet::io(),
        }
    }

    fn send_message_without_response(
        &self,
        _channel_id: u32,
        _message_id: u32,
        _message_data_len: usize,
    ) -> SbiRet {
        SbiRet::not_supported()
    }

    fn get_notification_events(&self, _channel_id: u32) -> SbiRet {
        SbiRet::not_supported()
    }
}
