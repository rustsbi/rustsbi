use core::sync::atomic::{AtomicUsize, Ordering};

use rustsbi::SbiRet;
use sbi_spec::binary::{SharedPtr, TriggerMask};

use crate::sbi::early_trap::{TrapInfo, csr_read_allow, csr_write_allow};

/// Implementation of SBI Debug Triggers (DBTR) extension.
///
/// The DBTR extension requires the RISC-V Sdtrig hardware debug trigger
/// interface. The prototyper probes `tselect` and `tdata1` directly to count
/// the triggers available on the calling hart, but does not model real trigger
/// configuration. Configuration requests remain unsupported, while
/// `num_triggers` reports the probed count and `set_shmem` records the
/// shared-memory pointer.
pub(crate) struct SbiDbtr;

// Sdtrig CSR numbers (RISC-V Debug specification). The `riscv` crate does not
// provide wrappers for these, so they are accessed with raw CSR instructions
// (see the `csr_read`/`csr_write` helpers below, same pattern as
// `riscv/spacemit_k1.rs`).
const CSR_TSELECT: u16 = 0x7a0;
const CSR_TDATA1: u16 = 0x7a1;
/// Not read by the probe; kept for reference.
#[allow(dead_code)]
const CSR_TDATA2: u16 = 0x7a2;
/// Not read by the probe; kept for reference.
#[allow(dead_code)]
const CSR_TDATA3: u16 = 0x7a3;
/// Not read by the probe; kept for reference.
#[allow(dead_code)]
const CSR_TINFO: u16 = 0x7a4;

/// Upper bound for the trigger walk; probing `tselect` 0..=255 covers at most
/// 256 triggers.
const SBI_DBTR_TRIG_MAX: usize = 255;

/// Cached trigger count; `usize::MAX` means "not probed yet".
static TRIG_MAX: AtomicUsize = AtomicUsize::new(usize::MAX);

/// Shared-memory physical address recorded by `set_shmem`.
static SHMEM_PTR: AtomicUsize = AtomicUsize::new(0);

/// Probes the Sdtrig hardware to count the number of debug triggers on the
/// calling hart.
///
/// Walks `tselect` from 0 upwards: writing an index and reading it back must
/// return the same value, otherwise the walk stops. A trigger counts only if
/// its `tdata1.type` field (bits 31:28) is non-zero.
fn probe_triggers() -> usize {
    let mut count = 0;
    for i in 0..=SBI_DBTR_TRIG_MAX {
        let mut trap = TrapInfo::default();
        unsafe { csr_write_allow::<CSR_TSELECT>(&mut trap, i) };
        if trap.mcause != usize::MAX {
            break;
        }

        let selected = unsafe { csr_read_allow::<CSR_TSELECT>(&mut trap) };
        if trap.mcause != usize::MAX || selected != i {
            break;
        }

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

/// Returns the number of debug triggers on the calling hart, probing the
/// Sdtrig hardware once and caching the result.
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
        // Prototyper does not model real trigger configuration, so these stay
        // not supported even when the hart has Sdtrig triggers.
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
