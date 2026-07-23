//! One contained expected-fault window shared by typed machine leaves.
//!
//! This module does not authorize an address or choose a CSR. Its crate-private
//! leaves execute one fixed instruction and return owned values or fault
//! metadata; responsibility-specific modules decide which leaves are exposed.

use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicUsize, Ordering};

use crate::config::HART_CAPACITY;

mod arch;

pub(crate) use arch::{load_byte, probe_csr, store_byte, swap_csr};

const FLAG_HYPERVISOR: usize = 1;

#[crate::mtest]
fn illegal_csr_fault_is_contained() {
    // SAFETY: 0xfff is a fixed reserved CSR selected by this lower test. The
    // expected-trap window accepts only illegal instruction and restores every
    // CSR it changes before returning an owned result.
    assert!(matches!(
        unsafe { probe_csr::<0xfff>() },
        ExpectedResult::Fault(fault) if fault.cause == 2
    ));
}

/// Machine-owned state for one current-hart expected-fault window.
///
/// Every field is integer-valued and assembly-addressable. The record contains
/// no pointer or reference to an ordinary trap frame or external memory.
#[repr(C, align(16))]
pub(super) struct ExpectedTrapRecord {
    armed: UnsafeCell<usize>,
    expected_cause: UnsafeCell<usize>,
    resume_pc: UnsafeCell<usize>,
    faulted: UnsafeCell<usize>,
    cause: UnsafeCell<usize>,
    tval: UnsafeCell<usize>,
    tval2: UnsafeCell<usize>,
    tinst: UnsafeCell<usize>,
    gva: UnsafeCell<usize>,
    flags: AtomicUsize,
    saved_mtvec: UnsafeCell<usize>,
    saved_mstatus: UnsafeCell<usize>,
    saved_mstatus_high: UnsafeCell<usize>,
}

// SAFETY: each record is selected only by its admitted dense hart index.
// Expected-fault assembly disables local interrupts and rejects reentry before
// mutating the current hart's record.
unsafe impl Sync for ExpectedTrapRecord {}

impl ExpectedTrapRecord {
    pub(super) const fn new() -> Self {
        Self {
            armed: UnsafeCell::new(0),
            expected_cause: UnsafeCell::new(0),
            resume_pc: UnsafeCell::new(0),
            faulted: UnsafeCell::new(0),
            cause: UnsafeCell::new(0),
            tval: UnsafeCell::new(0),
            tval2: UnsafeCell::new(0),
            tinst: UnsafeCell::new(0),
            gva: UnsafeCell::new(0),
            flags: AtomicUsize::new(0),
            saved_mtvec: UnsafeCell::new(0),
            saved_mstatus: UnsafeCell::new(0),
            saved_mstatus_high: UnsafeCell::new(0),
        }
    }

    /// Enables H-extension fault metadata after current-hart probing.
    ///
    /// # Safety
    ///
    /// The caller must be the state's current hart, with maskable machine
    /// interrupts disabled and before that hart enters a lower privilege mode.
    pub(super) unsafe fn enable_hypervisor_metadata(&self) {
        self.flags.store(FLAG_HYPERVISOR, Ordering::Release);
    }

    fn fault(&self) -> ExpectedFault {
        // SAFETY: the closed assembly window has restored machine state and
        // disarmed this current-hart record before returning to Rust.
        unsafe {
            ExpectedFault {
                cause: *self.cause.get(),
                value: *self.tval.get(),
                value2: *self.tval2.get(),
                instruction: *self.tinst.get(),
                guest_address: *self.gva.get() != 0,
            }
        }
    }
}

static EXPECTED_TRAPS: [ExpectedTrapRecord; HART_CAPACITY] =
    [const { ExpectedTrapRecord::new() }; HART_CAPACITY];

pub(super) fn current_record() -> Option<&'static ExpectedTrapRecord> {
    EXPECTED_TRAPS.get(super::current_index()?)
}

pub(super) fn enable_hypervisor_metadata(index: usize) -> bool {
    let Some(record) = EXPECTED_TRAPS.get(index) else {
        return false;
    };
    // SAFETY: current-hart preparation runs with machine interrupts disabled
    // before the first lower-mode entry and writes the immutable capability.
    unsafe { record.enable_hypervisor_metadata() };
    true
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ExpectedFault {
    pub(crate) cause: usize,
    pub(crate) value: usize,
    pub(crate) value2: usize,
    pub(crate) instruction: usize,
    pub(crate) guest_address: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ExpectedResult {
    Value(usize),
    Fault(ExpectedFault),
    Busy,
    Unavailable,
}

const ARMED_OFFSET: usize = core::mem::offset_of!(ExpectedTrapRecord, armed);
const EXPECTED_CAUSE_OFFSET: usize = core::mem::offset_of!(ExpectedTrapRecord, expected_cause);
const RESUME_PC_OFFSET: usize = core::mem::offset_of!(ExpectedTrapRecord, resume_pc);
const FAULTED_OFFSET: usize = core::mem::offset_of!(ExpectedTrapRecord, faulted);
const CAUSE_OFFSET: usize = core::mem::offset_of!(ExpectedTrapRecord, cause);
const TVAL_OFFSET: usize = core::mem::offset_of!(ExpectedTrapRecord, tval);
const TVAL2_OFFSET: usize = core::mem::offset_of!(ExpectedTrapRecord, tval2);
const TINST_OFFSET: usize = core::mem::offset_of!(ExpectedTrapRecord, tinst);
const GVA_OFFSET: usize = core::mem::offset_of!(ExpectedTrapRecord, gva);
const FLAGS_OFFSET: usize = core::mem::offset_of!(ExpectedTrapRecord, flags);
const SAVED_MTVEC_OFFSET: usize = core::mem::offset_of!(ExpectedTrapRecord, saved_mtvec);
const SAVED_MSTATUS_OFFSET: usize = core::mem::offset_of!(ExpectedTrapRecord, saved_mstatus);
const SAVED_MSTATUS_HIGH_OFFSET: usize =
    core::mem::offset_of!(ExpectedTrapRecord, saved_mstatus_high);

const _: () = assert!(ARMED_OFFSET == 0);
const _: () = assert!(core::mem::align_of::<ExpectedTrapRecord>() == 16);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_offsets_are_consecutive_words() {
        let word = core::mem::size_of::<usize>();
        for (index, offset) in [
            ARMED_OFFSET,
            EXPECTED_CAUSE_OFFSET,
            RESUME_PC_OFFSET,
            FAULTED_OFFSET,
            CAUSE_OFFSET,
            TVAL_OFFSET,
            TVAL2_OFFSET,
            TINST_OFFSET,
            GVA_OFFSET,
            FLAGS_OFFSET,
            SAVED_MTVEC_OFFSET,
            SAVED_MSTATUS_OFFSET,
            SAVED_MSTATUS_HIGH_OFFSET,
        ]
        .into_iter()
        .enumerate()
        {
            assert_eq!(offset, index * word);
        }
    }
}
