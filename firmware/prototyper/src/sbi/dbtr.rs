//! Debug-trigger support.
//!
//! # References
//!
//! - Specification: [RISC-V SBI DBTR extension](https://docs.riscv.org/reference/sbi/v3.0/ext-debug-triggers.html) —
//!   shared-memory layout and debug-trigger operations.

use core::mem::{align_of, size_of};
use core::sync::atomic::{AtomicUsize, Ordering};

use runtime::memory::{PhysAddr, PhysAddrRange, SupervisorMemory};
use rustsbi::SbiRet;
use sbi_spec::binary::{SharedPtr, TriggerMask};

use crate::sbi::early_trap::{TrapInfo, csr_read_allow, csr_write_allow};

/// Debug Triggers extension for harts with the RISC-V Sdtrig interface.
///
/// Trigger configuration is not supported.
pub(crate) struct SbiDbtr {
    supervisor_memory: &'static SupervisorMemory,
}

impl SbiDbtr {
    pub(crate) const fn new(supervisor_memory: &'static SupervisorMemory) -> Self {
        Self { supervisor_memory }
    }
}

// The `riscv` crate has no Sdtrig CSR wrappers, so probes use raw CSR helpers
// that contain illegal-instruction traps for absent CSRs.
const CSR_TSELECT: u16 = 0x7a0;
const CSR_TDATA1: u16 = 0x7a1;
// Bound the `tselect` walk independently of the hardware.
const MAX_PROBED_TRIGGERS: usize = 256;

// The trigger count is cached once; `usize::MAX` denotes an empty cache.
static CACHED_TRIGGER_COUNT: AtomicUsize = AtomicUsize::new(usize::MAX);

static SHMEM_ADDRESS: AtomicUsize = AtomicUsize::new(0);

fn probe_triggers() -> usize {
    let mut count = 0;
    // A selector is usable only if it reads back unchanged. A zero
    // `tdata1.type` field does not identify an implemented trigger.
    for index in 0..MAX_PROBED_TRIGGERS {
        let mut trap = TrapInfo::default();
        // SAFETY: firmware runs in M-mode, and `trap` remains valid for the call.
        unsafe { csr_write_allow::<CSR_TSELECT>(&mut trap, index) };
        if trap.mcause != usize::MAX {
            break;
        }

        // SAFETY: firmware runs in M-mode, and `trap` remains valid for the call.
        let selected = unsafe { csr_read_allow::<CSR_TSELECT>(&mut trap) };
        if trap.mcause != usize::MAX || selected != index {
            break;
        }

        // SAFETY: firmware runs in M-mode, and `trap` remains valid for the call.
        let tdata1 = unsafe { csr_read_allow::<CSR_TDATA1>(&mut trap) };
        if trap.mcause != usize::MAX {
            break;
        }

        if ((tdata1 >> (usize::BITS - 4)) & 0xf) != 0 {
            count += 1;
        }
    }
    count
}

fn cached_trigger_count() -> usize {
    let cached = CACHED_TRIGGER_COUNT.load(Ordering::Relaxed);
    if cached != usize::MAX {
        return cached;
    }
    let probed = probe_triggers();
    CACHED_TRIGGER_COUNT.store(probed, Ordering::Relaxed);
    probed
}

impl rustsbi::Dbtr for SbiDbtr {
    fn num_triggers(&self, trigger_data1: usize) -> usize {
        if trigger_data1 == 0 {
            cached_trigger_count()
        } else {
            // A nonzero `tdata1` request requires trigger-type filtering,
            // which this scaffolding does not implement.
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
