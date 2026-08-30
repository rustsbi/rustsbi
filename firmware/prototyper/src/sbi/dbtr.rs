use core::sync::atomic::{AtomicUsize, Ordering};

use rustsbi::SbiRet;
use sbi_spec::binary::{SharedPtr, TriggerMask};

use crate::sbi::early_trap::{TrapInfo, csr_read_allow, csr_write_allow};

/// Debug Triggers extension for harts with the RISC-V Sdtrig interface.
///
/// Trigger configuration is not supported.
pub(crate) struct SbiDbtr;

// The `riscv` crate has no Sdtrig CSR wrappers, so probes use raw CSR helpers
// that contain illegal-instruction traps for absent CSRs.
const CSR_TSELECT: u16 = 0x7a0;
const CSR_TDATA1: u16 = 0x7a1;
// Defined by Sdtrig but unused until trigger configuration is implemented.
#[allow(dead_code)]
const CSR_TDATA2: u16 = 0x7a2;
#[allow(dead_code)]
const CSR_TDATA3: u16 = 0x7a3;
#[allow(dead_code)]
const CSR_TINFO: u16 = 0x7a4;

// Bound the `tselect` walk to at most 256 triggers.
const SBI_DBTR_TRIG_MAX: usize = 255;

// The trigger count is cached once; `usize::MAX` denotes an empty cache.
static TRIG_MAX: AtomicUsize = AtomicUsize::new(usize::MAX);

static SHMEM_PTR: AtomicUsize = AtomicUsize::new(0);

fn probe_triggers() -> usize {
    let mut count = 0;
    // A selector is usable only if it reads back unchanged. A zero
    // `tdata1.type` field does not identify an implemented trigger.
    for i in 0..=SBI_DBTR_TRIG_MAX {
        let mut trap = TrapInfo::default();
        // SAFETY: firmware runs in M-mode, and `trap` remains valid for the call.
        unsafe { csr_write_allow::<CSR_TSELECT>(&mut trap, i) };
        if trap.mcause != usize::MAX {
            break;
        }

        // SAFETY: firmware runs in M-mode, and `trap` remains valid for the call.
        let selected = unsafe { csr_read_allow::<CSR_TSELECT>(&mut trap) };
        if trap.mcause != usize::MAX || selected != i {
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

fn num_triggers_probed() -> usize {
    let cached = TRIG_MAX.load(Ordering::Relaxed);
    if cached != usize::MAX {
        return cached;
    }
    let probed = probe_triggers();
    TRIG_MAX.store(probed, Ordering::Relaxed);
    probed
}

impl rustsbi::Dbtr for SbiDbtr {
    fn num_triggers(&self, trig_tdata1: usize) -> usize {
        if trig_tdata1 == 0 {
            num_triggers_probed()
        } else {
            // A nonzero `tdata1` request requires trigger-type filtering,
            // which this scaffolding does not implement.
            0
        }
    }

    fn set_shmem(&self, shmem: SharedPtr<u8>, flags: usize) -> SbiRet {
        if flags != 0 {
            return SbiRet::invalid_param();
        }

        let lo = shmem.phys_addr_lo();
        let hi = shmem.phys_addr_hi();
        if hi == usize::MAX && lo == usize::MAX {
            SHMEM_PTR.store(0, Ordering::Relaxed);
            return SbiRet::success(0);
        }

        let trigger_count = num_triggers_probed();
        if trigger_count == 0 {
            return SbiRet::not_supported();
        }

        if lo & (core::mem::size_of::<usize>() - 1) != 0 || hi != 0 {
            return SbiRet::invalid_param();
        }

        let Some(shmem_size) = trigger_count.checked_mul(4 * core::mem::size_of::<usize>()) else {
            return SbiRet::invalid_address();
        };
        if !crate::firmware::supervisor_writable(lo, shmem_size) {
            return SbiRet::invalid_address();
        }

        SHMEM_PTR.store(lo, Ordering::Relaxed);
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
