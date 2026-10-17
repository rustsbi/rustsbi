//! SBI DBTR trigger discovery and shared-memory registration.
//!
//! Specification:
//! <https://docs.riscv.org/reference/sbi/v3.0/ext-debug-triggers.html>.

use core::mem::{align_of, size_of};
use core::sync::atomic::{AtomicUsize, Ordering};

use runtime::{
    debug,
    memory::{PhysAddr, PhysAddrRange, SupervisorMemory},
    rustsbi::{Dbtr, SbiRet},
};
use sbi_spec::binary::{SharedPtr, TriggerMask};

/// SBI DBTR implementation for harts with the RISC-V Sdtrig interface.
///
/// Only unfiltered trigger discovery and shared-memory registration are
/// supported.
pub(crate) struct SbiDbtr {
    supervisor_memory: &'static SupervisorMemory,
}

impl SbiDbtr {
    pub(crate) const fn new(supervisor_memory: &'static SupervisorMemory) -> Self {
        Self { supervisor_memory }
    }
}

// The trigger count is cached once; `usize::MAX` denotes an empty cache.
static CACHED_TRIGGER_COUNT: AtomicUsize = AtomicUsize::new(usize::MAX);

static SHMEM_ADDRESS: AtomicUsize = AtomicUsize::new(0);

fn cached_trigger_count() -> usize {
    let cached = CACHED_TRIGGER_COUNT.load(Ordering::Relaxed);
    if cached != usize::MAX {
        return cached;
    }
    let probed = debug::trigger_count();
    CACHED_TRIGGER_COUNT.store(probed, Ordering::Relaxed);
    probed
}

impl Dbtr for SbiDbtr {
    fn num_triggers(&self, trigger_data1: usize) -> usize {
        if trigger_data1 == 0 {
            cached_trigger_count()
        } else {
            // Nonzero tdata1 requests require trigger-type filtering,
            // which this implementation does not support.
            0
        }
    }

    fn set_shmem(&self, shared_memory: SharedPtr<u8>, flags: usize) -> SbiRet {
        if flags != 0 {
            return SbiRet::invalid_param();
        }

        let start = PhysAddr::new(shared_memory.phys_addr_lo());
        let address_high = shared_memory.phys_addr_hi();
        if address_high == usize::MAX && start.as_usize() == usize::MAX {
            SHMEM_ADDRESS.store(0, Ordering::Relaxed);
            return SbiRet::success(0);
        }

        let trigger_count = cached_trigger_count();
        if trigger_count == 0 {
            return SbiRet::not_supported();
        }

        if !start.is_aligned_to(align_of::<usize>()) || address_high != 0 {
            return SbiRet::invalid_param();
        }

        let Some(shared_memory_size) = trigger_count.checked_mul(4 * size_of::<usize>()) else {
            return SbiRet::invalid_address();
        };
        let Ok(range) = PhysAddrRange::from_start_len(start, shared_memory_size) else {
            return SbiRet::invalid_address();
        };
        if self.supervisor_memory.check_range(range).is_err() {
            return SbiRet::invalid_address();
        }

        SHMEM_ADDRESS.store(start.as_usize(), Ordering::Relaxed);
        SbiRet::success(0)
    }

    fn read_triggers(&self, _trig_idx_base: usize, _trig_count: usize) -> SbiRet {
        SbiRet::not_supported()
    }

    fn install_triggers(&self, _trig_count: usize) -> SbiRet {
        SbiRet::not_supported()
    }

    fn update_triggers(&self, _trig_count: usize) -> SbiRet {
        SbiRet::not_supported()
    }

    fn uninstall_triggers(&self, _triggers: TriggerMask) -> SbiRet {
        SbiRet::not_supported()
    }

    fn enable_triggers(&self, _triggers: TriggerMask) -> SbiRet {
        SbiRet::not_supported()
    }

    fn disable_triggers(&self, _triggers: TriggerMask) -> SbiRet {
        SbiRet::not_supported()
    }
}
