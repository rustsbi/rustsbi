//! Steal-time shared memory.
//!
//! # References
//!
//! - Specification: [RISC-V SBI STA extension](https://docs.riscv.org/reference/sbi/v3.0/ext-steal-time.html) —
//!   shared-memory layout, alignment, and update protocol.

#![forbid(unsafe_code)]

use runtime::memory::{PhysAddr, SupervisorMemory};
use rustsbi::SbiRet;
use sbi_spec::binary::SharedPtr;

const SHMEM_SIZE: usize = 64;

/// Steal-time Accounting extension using supervisor-provided shared memory.
pub(crate) struct SbiSta {
    supervisor_memory: &'static SupervisorMemory,
}

impl SbiSta {
    pub(crate) const fn new(supervisor_memory: &'static SupervisorMemory) -> Self {
        Self { supervisor_memory }
    }
}

impl rustsbi::Sta for SbiSta {
    fn set_shmem(&self, shared_memory: SharedPtr<[u8; SHMEM_SIZE]>, flags: usize) -> SbiRet {
        if flags != 0 {
            return SbiRet::invalid_param();
        }

        let start = PhysAddr::new(shared_memory.phys_addr_lo());
        let address_high = shared_memory.phys_addr_hi();

        // All-ones shared pointer disables steal-time reporting.
        if address_high == usize::MAX && start.as_usize() == usize::MAX {
            return SbiRet::success(0);
        }

        // STA requires a 64-byte aligned native physical address.
        if !start.is_aligned_to(SHMEM_SIZE) || address_high != 0 {
            return SbiRet::invalid_param();
        }

        // STA transfers the shared area to firmware for initialization.
        if self
            .supervisor_memory
            .write(start, &[0; SHMEM_SIZE])
            .is_err()
        {
            return SbiRet::invalid_address();
        }
        core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);

        SbiRet::success(0)
    }
}
