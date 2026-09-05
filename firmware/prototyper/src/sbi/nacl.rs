//! Nested-acceleration shared memory.
//!
//! # References
//!
//! - Specification: [RISC-V SBI NACL extension](https://docs.riscv.org/reference/sbi/v3.0/ext-nested-acceleration.html) —
//!   shared-memory layout, alignment, and initialization.

#![forbid(unsafe_code)]

use runtime::memory::{PhysAddr, SupervisorMemory};
use rustsbi::SbiRet;
use sbi_spec::binary::SharedPtr;
use sbi_spec::nacl::shmem_size::NATIVE;

const SHMEM_PAGE_SHIFT: u32 = 12;
const SHMEM_ALIGNMENT: usize = 1usize << SHMEM_PAGE_SHIFT;

/// Nested Acceleration extension for harts with the RISC-V H extension.
///
/// No emulated features are advertised because virtualization is provided by
/// hardware.
pub(crate) struct SbiNacl {
    supervisor_memory: &'static SupervisorMemory,
}

impl SbiNacl {
    pub(crate) const fn new(supervisor_memory: &'static SupervisorMemory) -> Self {
        Self { supervisor_memory }
    }
}

impl rustsbi::Nacl for SbiNacl {
    fn probe_feature(&self, _feature_id: u32) -> SbiRet {
        SbiRet::success(0)
    }

    fn set_shmem(&self, shared_memory: SharedPtr<[u8; NATIVE]>, flags: usize) -> SbiRet {
        // The flags field is reserved and must be zero.
        if flags != 0 {
            return SbiRet::invalid_param();
        }

        let start = PhysAddr::new(shared_memory.phys_addr_lo());
        let address_high = shared_memory.phys_addr_hi();

        // An all-ones pointer disables the NACL features.
        if address_high == usize::MAX && start.as_usize() == usize::MAX {
            return SbiRet::success(0);
        }

        // NACL shared memory must be page-aligned and addressable by usize.
        if !start.is_aligned_to(SHMEM_ALIGNMENT) || address_high != 0 {
            return SbiRet::invalid_param();
        }

        // NACL transfers the shared area to firmware for initialization.
        if self.supervisor_memory.fill_zeros(start, NATIVE).is_err() {
            return SbiRet::invalid_address();
        }
        core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);

        SbiRet::success(0)
    }

    fn sync_csr(&self, _csr_num: usize) -> SbiRet {
        SbiRet::not_supported()
    }

    fn sync_hfence(&self, _entry_index: usize) -> SbiRet {
        SbiRet::not_supported()
    }

    fn sync_sret(&self) -> SbiRet {
        SbiRet::not_supported()
    }

    fn _rustsbi_probe(&self) -> usize {
        sbi_spec::base::UNAVAILABLE_EXTENSION
    }
}
