//! SBI DBTR trigger discovery and shared-memory registration.
//!
//! Specification:
//! <https://docs.riscv.org/reference/sbi/v3.0/ext-debug-triggers.html>.

#![forbid(unsafe_code)]

use core::mem::{align_of, size_of};
use core::sync::atomic::{AtomicUsize, Ordering};

use runtime::{
    Result, debug,
    hart::{HartId, PerHart},
    memory::{PhysAddr, PhysAddrRange, SupervisorMemory},
    rustsbi::{Dbtr, SbiRet},
};
use sbi_spec::binary::{SharedPtr, TriggerMask};
use spin::Once;

const TRIGGER_ENTRY_WORDS: usize = 4;
const TRIGGER_ENTRY_BYTES: usize = TRIGGER_ENTRY_WORDS * size_of::<usize>();
const SHMEM_DISABLED: usize = usize::MAX;

/// SBI DBTR implementation for harts with the RISC-V Sdtrig interface.
///
/// Supports per-hart shared-memory registration and trigger discovery/readback.
pub(crate) struct SbiDbtr {
    shmem: PerHart<AtomicUsize>,
    trigger_counts: PerHart<Once<usize>>,
    supervisor_memory: &'static SupervisorMemory,
}

impl SbiDbtr {
    pub(crate) fn new(supervisor_memory: &'static SupervisorMemory) -> Result<Self> {
        Ok(Self {
            shmem: PerHart::new(|_| AtomicUsize::new(SHMEM_DISABLED))?,
            trigger_counts: PerHart::new(|_| Once::new())?,
            supervisor_memory,
        })
    }
}

impl Dbtr for SbiDbtr {
    fn num_triggers(&self, trigger_data1: usize) -> usize {
        if trigger_data1 == 0 {
            HartId::current().map_or(0, |hart| {
                *self
                    .trigger_counts
                    .get(hart)
                    .call_once(debug::trigger_count)
            })
        } else {
            // Nonzero tdata1 requests require trigger-type filtering,
            // which this implementation does not support.
            0
        }
    }

    fn set_shmem(&self, shared_memory: SharedPtr<u8>, flags: usize) -> SbiRet {
        let Ok(hart) = HartId::current() else {
            return SbiRet::failed();
        };
        if flags != 0 {
            return SbiRet::invalid_param();
        }

        let start = PhysAddr::new(shared_memory.phys_addr_lo());
        let address_high = shared_memory.phys_addr_hi();
        let shmem = self.shmem.get(hart);
        if address_high == usize::MAX && start.as_usize() == usize::MAX {
            shmem.store(SHMEM_DISABLED, Ordering::Release);
            return SbiRet::success(0);
        }

        // The DBTR shared-memory base address is passed as two XLEN halves and
        // may be wider than XLEN bits. The alignment is decided by the lower
        // XLEN bits, which `start` holds.
        if !start.is_aligned_to(align_of::<usize>()) {
            return SbiRet::invalid_param();
        }
        // An address above the native physical-address space cannot compose a
        // registered supervisor-memory range, so it is inaccessible rather
        // than an invalid parameter.
        if address_high != 0 {
            return SbiRet::invalid_address();
        }

        let trigger_count = *self
            .trigger_counts
            .get(hart)
            .call_once(debug::trigger_count);
        let shared_memory_size = if trigger_count == 0 {
            // Still require at least one accessible word at `start` so invalid or M-mode
            // addresses are rejected even when no hardware triggers are exposed.
            align_of::<usize>()
        } else {
            let Some(size) = trigger_count.checked_mul(TRIGGER_ENTRY_BYTES) else {
                return SbiRet::invalid_address();
            };
            size
        };
        let Ok(range) = PhysAddrRange::from_start_len(start, shared_memory_size) else {
            return SbiRet::invalid_address();
        };
        if self.supervisor_memory.check_range(range).is_err() {
            return SbiRet::invalid_address();
        }

        shmem.store(start.as_usize(), Ordering::Release);
        SbiRet::success(0)
    }

    fn read_triggers(&self, trig_idx_base: usize, trig_count: usize) -> SbiRet {
        let Ok(hart) = HartId::current() else {
            return SbiRet::failed();
        };
        let shmem = self.shmem.get(hart).load(Ordering::Acquire);
        if shmem == SHMEM_DISABLED {
            return SbiRet::no_shmem();
        }

        let trig_max = *self
            .trigger_counts
            .get(hart)
            .call_once(debug::trigger_count);
        if trig_idx_base >= trig_max
            || trig_idx_base
                .checked_add(trig_count)
                .is_none_or(|end| end > trig_max)
        {
            return SbiRet::bad_range();
        }

        let Some(total_size) = trig_count.checked_mul(TRIGGER_ENTRY_BYTES) else {
            return SbiRet::invalid_address();
        };

        // Dynamic triggers are unimplemented, so report zeroed unmapped entries
        // without exposing external debugger or M-mode trigger CSRs.
        if self
            .supervisor_memory
            .fill_zeros(PhysAddr::new(shmem), total_size)
            .is_err()
        {
            return SbiRet::invalid_address();
        }

        SbiRet::success(0)
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
