//! Message-proxy shared memory.
//!
//! # References
//!
//! - Specification: [RISC-V SBI MPXY extension](https://docs.riscv.org/reference/sbi/v3.0/ext-mpxy.html) —
//!   shared-memory layout, alignment, and transfer semantics.

#![forbid(unsafe_code)]

use core::sync::atomic::{AtomicUsize, Ordering};

use runtime::memory::{PhysAddr, PhysAddrRange, SupervisorMemory};
use rustsbi::SbiRet;
use sbi_spec::binary::SharedPtr;

// The MPXY ABI requires at least one 4096-byte page and requires the reported
// size to be a multiple of that page. No channels are exposed yet, so the
// minimum is sufficient.
const PAGE_SHIFT: u32 = 12;
const SHARED_MEMORY_SIZE: usize = 1usize << PAGE_SHIFT;

/// Message Proxy extension with per-hart shared memory but no message channels.
pub(crate) struct SbiMpxy {
    shmem: [AtomicUsize; crate::cfg::NUM_HART_MAX],
    supervisor_memory: &'static SupervisorMemory,
}

impl SbiMpxy {
    pub(crate) const fn new(supervisor_memory: &'static SupervisorMemory) -> Self {
        Self {
            shmem: [const { AtomicUsize::new(0) }; crate::cfg::NUM_HART_MAX],
            supervisor_memory,
        }
    }

    #[inline]
    fn current_shmem(&self) -> &AtomicUsize {
        &self.shmem[crate::riscv::current_hartid()]
    }
}

impl rustsbi::Mpxy for SbiMpxy {
    fn get_shmem_size(&self) -> usize {
        SHARED_MEMORY_SIZE
    }

    fn set_shmem(&self, shared_memory: SharedPtr<u8>, flags: usize) -> SbiRet {
        if flags != 0 {
            return SbiRet::invalid_param();
        }

        let start = PhysAddr::new(shared_memory.phys_addr_lo());
        let address_high = shared_memory.phys_addr_hi();
        if start.as_usize() == usize::MAX && address_high == usize::MAX {
            self.current_shmem().store(0, Ordering::Release);
            return SbiRet::success(0);
        }

        if !start.is_aligned_to(SHARED_MEMORY_SIZE) || address_high != 0 {
            return SbiRet::invalid_param();
        }
        let Ok(range) = PhysAddrRange::from_start_len(start, SHARED_MEMORY_SIZE) else {
            return SbiRet::invalid_address();
        };
        if self.supervisor_memory.check_range(range).is_err() {
            return SbiRet::invalid_address();
        }

        self.current_shmem()
            .store(start.as_usize(), Ordering::Release);
        SbiRet::success(0)
    }

    fn get_channel_ids(&self, _start_index: u32) -> SbiRet {
        if self.current_shmem().load(Ordering::Acquire) == 0 {
            return SbiRet::no_shmem();
        }
        SbiRet::failed()
    }

    fn read_attributes(
        &self,
        _channel_id: u32,
        _base_attribute_id: u32,
        _attribute_count: u32,
        _output: SharedPtr<u8>,
    ) -> SbiRet {
        SbiRet::failed()
    }

    fn write_attributes(
        &self,
        _channel_id: u32,
        _base_attribute_id: u32,
        _attribute_count: u32,
        _input: SharedPtr<u8>,
    ) -> SbiRet {
        SbiRet::failed()
    }

    fn send_message_with_response(
        &self,
        _channel_id: u32,
        _message_id: u32,
        _message_data_len: usize,
    ) -> SbiRet {
        SbiRet::failed()
    }

    fn send_message_without_response(
        &self,
        _channel_id: u32,
        _message_id: u32,
        _message_data_len: usize,
    ) -> SbiRet {
        SbiRet::failed()
    }

    fn get_notification_events(&self, _channel_id: u32) -> SbiRet {
        SbiRet::failed()
    }
}
