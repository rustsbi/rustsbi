//! Message-proxy shared memory.
//!
//! # References
//!
//! - Specification: [RISC-V SBI MPXY extension](https://docs.riscv.org/reference/sbi/v3.0/ext-mpxy.html) —
//!   shared-memory layout, alignment, and transfer semantics.

#![forbid(unsafe_code)]

use core::sync::atomic::{AtomicUsize, Ordering};

use runtime::{
    Result,
    hart::{HartId, PerHart},
    memory::{PhysAddr, PhysAddrRange, SupervisorMemory},
    rustsbi::{Mpxy, SbiRet},
};
use sbi_spec::binary::SharedPtr;

// The MPXY ABI requires at least one 4096-byte page and requires the reported
// size to be a multiple of that page. No channels are exposed yet, so the
// minimum is sufficient.
const PAGE_SHIFT: u32 = 12;
const SHARED_MEMORY_SIZE: usize = 1usize << PAGE_SHIFT;
const SHMEM_DISABLED: usize = usize::MAX;
const FLAG_OVERWRITE_RETURN: usize = 1;
const CHANNEL_IDS_HEADER_BYTES: usize = 8;
const OVERWRITE_RETURN_BYTES: usize = 2 * core::mem::size_of::<usize>();

/// Message Proxy extension with per-hart shared memory but no message channels.
pub(crate) struct SbiMpxy {
    shmem: PerHart<AtomicUsize>,
    supervisor_memory: &'static SupervisorMemory,
}

impl SbiMpxy {
    pub(crate) fn new(supervisor_memory: &'static SupervisorMemory) -> Result<Self> {
        Ok(Self {
            shmem: PerHart::new(|_| AtomicUsize::new(SHMEM_DISABLED))?,
            supervisor_memory,
        })
    }

    #[inline]
    fn check_shmem(&self) -> SbiRet {
        let Ok(hart) = HartId::current() else {
            return SbiRet::failed();
        };
        if self.shmem.get(hart).load(Ordering::Acquire) == SHMEM_DISABLED {
            return SbiRet::no_shmem();
        }
        SbiRet::success(0)
    }
}

fn encode_overwrite_return(prev: usize) -> [u8; OVERWRITE_RETURN_BYTES] {
    let (prev_lo, prev_hi) = if prev == SHMEM_DISABLED {
        (usize::MAX, usize::MAX)
    } else {
        (prev, 0)
    };
    let word_size = core::mem::size_of::<usize>();
    let mut encoded = [0u8; OVERWRITE_RETURN_BYTES];
    encoded[..word_size].copy_from_slice(&prev_lo.to_le_bytes());
    encoded[word_size..].copy_from_slice(&prev_hi.to_le_bytes());
    encoded
}

impl Mpxy for SbiMpxy {
    fn get_shmem_size(&self) -> usize {
        SHARED_MEMORY_SIZE
    }

    fn set_shmem(&self, shared_memory: SharedPtr<u8>, flags: usize) -> SbiRet {
        let Ok(hart) = HartId::current() else {
            return SbiRet::failed();
        };
        if flags > FLAG_OVERWRITE_RETURN {
            return SbiRet::invalid_param();
        }

        let start = PhysAddr::new(shared_memory.phys_addr_lo());
        let address_high = shared_memory.phys_addr_hi();
        let shmem = self.shmem.get(hart);
        if start.as_usize() == usize::MAX && address_high == usize::MAX {
            shmem.store(SHMEM_DISABLED, Ordering::Release);
            return SbiRet::success(0);
        }

        // The MPXY shared-memory base address is passed as two XLEN halves and
        // may be wider than XLEN bits. The alignment is decided by the lower
        // XLEN bits, which `start` holds.
        if !start.is_aligned_to(SHARED_MEMORY_SIZE) {
            return SbiRet::invalid_param();
        }
        // An address above the native physical-address space cannot compose a
        // registered supervisor-memory range, so it is inaccessible rather
        // than an invalid parameter.
        if address_high != 0 {
            return SbiRet::invalid_address();
        }
        let Ok(range) = PhysAddrRange::from_start_len(start, SHARED_MEMORY_SIZE) else {
            return SbiRet::invalid_address();
        };
        if self.supervisor_memory.check_range(range).is_err() {
            return SbiRet::invalid_address();
        }

        if flags == FLAG_OVERWRITE_RETURN {
            let prev = shmem.load(Ordering::Acquire);
            let encoded = encode_overwrite_return(prev);
            if self.supervisor_memory.write(start, &encoded).is_err() {
                return SbiRet::invalid_address();
            }
        }

        shmem.store(start.as_usize(), Ordering::Release);
        SbiRet::success(0)
    }

    fn get_channel_ids(&self, start_index: u32) -> SbiRet {
        let Ok(hart) = HartId::current() else {
            return SbiRet::failed();
        };
        let shmem = self.shmem.get(hart).load(Ordering::Acquire);
        if shmem == SHMEM_DISABLED {
            return SbiRet::no_shmem();
        }
        if start_index != 0 {
            return SbiRet::invalid_param();
        }
        // Zero channels are exposed, so `remaining` and `returned` are both 0.
        if self
            .supervisor_memory
            .fill_zeros(PhysAddr::new(shmem), CHANNEL_IDS_HEADER_BYTES)
            .is_err()
        {
            return SbiRet::failed();
        }
        SbiRet::success(0)
    }

    fn read_attributes(
        &self,
        _channel_id: u32,
        _base_attribute_id: u32,
        _attribute_count: u32,
        _output: SharedPtr<u8>,
    ) -> SbiRet {
        let ret = self.check_shmem();
        if ret.is_err() {
            return ret;
        }
        // TODO: add actual MPXY extension support.
        SbiRet::not_supported()
    }

    fn write_attributes(
        &self,
        _channel_id: u32,
        _base_attribute_id: u32,
        _attribute_count: u32,
        _input: SharedPtr<u8>,
    ) -> SbiRet {
        let ret = self.check_shmem();
        if ret.is_err() {
            return ret;
        }
        // TODO: add actual MPXY extension support.
        SbiRet::not_supported()
    }

    fn send_message_with_response(
        &self,
        _channel_id: u32,
        _message_id: u32,
        _message_data_len: usize,
    ) -> SbiRet {
        let ret = self.check_shmem();
        if ret.is_err() {
            return ret;
        }
        // TODO: add actual MPXY extension support.
        SbiRet::not_supported()
    }

    fn send_message_without_response(
        &self,
        _channel_id: u32,
        _message_id: u32,
        _message_data_len: usize,
    ) -> SbiRet {
        let ret = self.check_shmem();
        if ret.is_err() {
            return ret;
        }
        // TODO: add actual MPXY extension support.
        SbiRet::not_supported()
    }

    fn get_notification_events(&self, _channel_id: u32) -> SbiRet {
        let ret = self.check_shmem();
        if ret.is_err() {
            return ret;
        }
        // TODO: add actual MPXY extension support.
        SbiRet::not_supported()
    }
}
