use core::sync::atomic::{AtomicUsize, Ordering};

use rustsbi::SbiRet;
use sbi_spec::binary::SharedPtr;

const SHMEM_SIZE: usize = 4096;

/// Minimal SBI Message Proxy extension implementation.
///
/// The prototyper has no discovered message-channel backend yet, so it only
/// accepts per-hart shared-memory setup and reports no usable channels.
pub(crate) struct SbiMpxy {
    shmem: [AtomicUsize; crate::cfg::NUM_HART_MAX],
}

impl SbiMpxy {
    pub(crate) const fn new() -> Self {
        Self {
            shmem: [const { AtomicUsize::new(0) }; crate::cfg::NUM_HART_MAX],
        }
    }

    #[inline]
    fn shmem_hart(&self) -> &AtomicUsize {
        &self.shmem[crate::riscv::current_hartid()]
    }
}

impl rustsbi::Mpxy for SbiMpxy {
    fn get_shmem_size(&self) -> usize {
        SHMEM_SIZE
    }

    fn set_shmem(&self, shmem: SharedPtr<u8>, flags: usize) -> SbiRet {
        if flags != 0 {
            return SbiRet::invalid_param();
        }

        let lo = shmem.phys_addr_lo();
        let hi = shmem.phys_addr_hi();
        if lo == usize::MAX && hi == usize::MAX {
            self.shmem_hart().store(0, Ordering::Release);
            return SbiRet::success(0);
        }

        if lo & (SHMEM_SIZE - 1) != 0 || hi != 0 {
            return SbiRet::invalid_param();
        }
        if !crate::firmware::supervisor_writable(lo, SHMEM_SIZE) {
            return SbiRet::invalid_address();
        }

        self.shmem_hart().store(lo, Ordering::Release);
        SbiRet::success(0)
    }

    fn get_channel_ids(&self, _start_index: u32) -> SbiRet {
        if self.shmem_hart().load(Ordering::Acquire) == 0 {
            return SbiRet::no_shmem();
        }
        SbiRet::not_supported()
    }

    fn read_attributes(
        &self,
        _channel_id: u32,
        _base_attribute_id: u32,
        _attribute_count: u32,
        _output: SharedPtr<u8>,
    ) -> SbiRet {
        SbiRet::not_supported()
    }

    fn write_attributes(
        &self,
        _channel_id: u32,
        _base_attribute_id: u32,
        _attribute_count: u32,
        _input: SharedPtr<u8>,
    ) -> SbiRet {
        SbiRet::not_supported()
    }

    fn send_message_with_response(
        &self,
        _channel_id: u32,
        _message_id: u32,
        _message_data_len: usize,
    ) -> SbiRet {
        SbiRet::not_supported()
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
