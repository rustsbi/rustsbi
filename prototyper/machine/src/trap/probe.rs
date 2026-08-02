//! One contained expected-fault window shared by typed machine leaves.
//!
//! This module does not authorize an address or choose a CSR. Its crate-private
//! leaves execute one fixed instruction and return owned values or fault
//! metadata; responsibility-specific modules decide which leaves are exposed.

use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicUsize, Ordering};

use crate::config::HART_CAPACITY;

const MSTATUS_MIE: usize = 1 << 3;
const MSTATUS_MPRV: usize = 1 << 17;
#[cfg(target_pointer_width = "64")]
const MSTATUS_MDT_RV64: usize = 1 << 42;
#[cfg(target_pointer_width = "32")]
const MSTATUS_MDT_RV64: usize = 0;
const MSTATUSH_MDT_RV32: usize = 1 << 10;

#[cfg(target_pointer_width = "64")]
macro_rules! load_word {
    ($register:literal, $field:literal) => {
        concat!("ld ", $register, ", {", $field, "}(a3)")
    };
}

#[cfg(target_pointer_width = "32")]
macro_rules! load_word {
    ($register:literal, $field:literal) => {
        concat!("lw ", $register, ", {", $field, "}(a3)")
    };
}

#[cfg(target_pointer_width = "64")]
macro_rules! store_word {
    ($register:literal, $field:literal) => {
        concat!("sd ", $register, ", {", $field, "}(a3)")
    };
}

#[cfg(target_pointer_width = "32")]
macro_rules! store_word {
    ($register:literal, $field:literal) => {
        concat!("sw ", $register, ", {", $field, "}(a3)")
    };
}

macro_rules! perform_probe {
    ($record:expr, $expected:literal, $access:literal, $($access_operand:tt)*) => {{
        let mut value: usize;
        let status: usize;
        let record = $record as *const ExpectedTrapRecord as *mut ExpectedTrapRecord;
        // SAFETY: record is current-hart state. Assembly disables MIE, rejects
        // reentry, initializes the record, installs the private vector, executes
        // exactly one access, restores every changed CSR, and returns integers.
        unsafe {
            core::arch::asm!(
                "csrr t0, mstatus",
                store_word!("t0", "saved_mstatus"),
                ".if {word_size} == 4",
                "csrr t2, mstatush",
                store_word!("t2", "saved_mstatus_high"),
                ".else",
                store_word!("zero", "saved_mstatus_high"),
                ".endif",
                "li t1, {mprv}",
                "and t2, t0, t1",
                "bnez t2, 90f",
                ".if {word_size} == 8",
                "li t1, {mdt_rv64}",
                "and t2, t0, t1",
                ".else",
                load_word!("t2", "saved_mstatus_high"),
                "li t1, {mdt_rv32}",
                "and t2, t2, t1",
                ".endif",
                "bnez t2, 90f",
                "li t1, {mie}",
                "csrc mstatus, t1",
                load_word!("t1", "armed"),
                "bnez t1, 80f",
                "li t1, 1",
                store_word!("t1", "armed"),
                "csrr t1, mtvec",
                store_word!("t1", "saved_mtvec"),
                "li t1, {expected_cause_value}",
                store_word!("t1", "expected_cause"),
                store_word!("zero", "faulted"),
                store_word!("zero", "cause"),
                store_word!("zero", "tval"),
                store_word!("zero", "tval2"),
                store_word!("zero", "tinst"),
                store_word!("zero", "gva"),
                "lla a4, 20f",
                store_word!("a4", "resume_pc"),
                "lla t1, {expected_entry}",
                "csrw mtvec, t1",
                $access,
                "li {status}, 0",
                "j 30f",
                "20:",
                load_word!("t0", "faulted"),
                "li t1, 1",
                "bne t0, t1, 90f",
                load_word!("t0", "cause"),
                "li t1, {expected_cause_value}",
                "bne t0, t1, 90f",
                "li {value}, 0",
                "li {status}, 1",
                "30:",
                load_word!("t0", "saved_mtvec"),
                "csrw mtvec, t0",
                ".if {word_size} == 4",
                load_word!("t0", "saved_mstatus_high"),
                "csrw mstatush, t0",
                ".endif",
                load_word!("t0", "saved_mstatus"),
                "li t1, {mie}",
                "not t1, t1",
                "and t2, t0, t1",
                "csrw mstatus, t2",
                store_word!("zero", "armed"),
                "csrw mstatus, t0",
                "j 95f",
                "80:",
                "li {value}, 0",
                "li {status}, 2",
                "csrw mstatus, t0",
                "j 95f",
                "90:",
                "csrw mie, zero",
                "91:",
                "wfi",
                "j 91b",
                "95:",
                value = lateout(reg) value,
                status = lateout(reg) status,
                expected_entry = sym __rustsbi_prototyper_expected_trap,
                expected_cause_value = const $expected,
                word_size = const core::mem::size_of::<usize>(),
                mie = const MSTATUS_MIE,
                mprv = const MSTATUS_MPRV,
                mdt_rv64 = const MSTATUS_MDT_RV64,
                mdt_rv32 = const MSTATUSH_MDT_RV32,
                armed = const ARMED_OFFSET,
                expected_cause = const EXPECTED_CAUSE_OFFSET,
                resume_pc = const RESUME_PC_OFFSET,
                faulted = const FAULTED_OFFSET,
                cause = const CAUSE_OFFSET,
                tval = const TVAL_OFFSET,
                tval2 = const TVAL2_OFFSET,
                tinst = const TINST_OFFSET,
                gva = const GVA_OFFSET,
                saved_mtvec = const SAVED_MTVEC_OFFSET,
                saved_mstatus = const SAVED_MSTATUS_OFFSET,
                saved_mstatus_high = const SAVED_MSTATUS_HIGH_OFFSET,
                inout("a3") record => _,
                lateout("a4") _,
                lateout("t0") _,
                lateout("t1") _,
                lateout("t2") _,
                $($access_operand)*
                options(nostack),
            );
        }
        match status {
            0 => ExpectedResult::Value(value),
            1 => ExpectedResult::Fault($record.fault()),
            2 => ExpectedResult::Busy,
            _ => crate::trap::abort(),
        }
    }};
}

const FLAG_HYPERVISOR: usize = 1;

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
    fn enable_hypervisor_metadata(&self) {
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
    record.enable_hypervisor_metadata();
    true
}

pub(super) fn hypervisor_metadata_available(index: usize) -> bool {
    EXPECTED_TRAPS
        .get(index)
        .is_some_and(|record| record.flags.load(Ordering::Acquire) & FLAG_HYPERVISOR != 0)
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

/// Reads one fixed CSR selected by a responsibility-specific wrapper.
pub(crate) unsafe fn probe_csr<const CSR: u16>() -> ExpectedResult {
    let Some(record) = current_record() else {
        return ExpectedResult::Unavailable;
    };
    perform_probe!(record, 2, "csrr {value}, {csr}", csr = const CSR,)
}

/// Atomically replaces one fixed CSR and returns its previous value.
pub(crate) unsafe fn swap_csr<const CSR: u16>(value: usize) -> ExpectedResult {
    let Some(record) = current_record() else {
        return ExpectedResult::Unavailable;
    };
    perform_probe!(
        record,
        2,
        "csrrw {value}, {csr}, {replacement}",
        csr = const CSR,
        replacement = in(reg) value,
    )
}

/// Loads one already-authorized physical byte.
pub(crate) unsafe fn load_byte(address: usize) -> ExpectedResult {
    let Some(record) = current_record() else {
        return ExpectedResult::Unavailable;
    };
    perform_probe!(
        record,
        5,
        "lbu {value}, 0({address})",
        address = in(reg) address,
    )
}

/// Stores one already-authorized physical byte.
pub(crate) unsafe fn store_byte(address: usize, byte: u8) -> ExpectedResult {
    let Some(record) = current_record() else {
        return ExpectedResult::Unavailable;
    };
    perform_probe!(
        record,
        7,
        "sb {byte}, 0({address})\nli {value}, 0",
        address = in(reg) address,
        byte = in(reg) usize::from(byte),
    )
}

unsafe extern "C" {
    fn __rustsbi_prototyper_expected_trap();
}

core::arch::global_asm!(
    include_str!("probe.S"),
    word_size = const core::mem::size_of::<usize>(),
    armed = const ARMED_OFFSET,
    resume_pc = const RESUME_PC_OFFSET,
    faulted = const FAULTED_OFFSET,
    cause = const CAUSE_OFFSET,
    tval = const TVAL_OFFSET,
    tval2 = const TVAL2_OFFSET,
    tinst = const TINST_OFFSET,
    gva = const GVA_OFFSET,
    flags = const FLAGS_OFFSET,
    flag_hypervisor = const FLAG_HYPERVISOR,
);

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
