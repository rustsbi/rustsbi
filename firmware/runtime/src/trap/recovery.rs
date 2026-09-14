//! The expected-fault recovery mechanism for guarded machine operations.
//!
//! A guarded operation publishes a [`RecoveryRecord`] through `mscratch`
//! and switches `mtvec` to the private `recovery_entry` for the duration of
//! exactly one guarded instruction. The entry accepts the fault only on an
//! exact record/origin/PC/cause match; every mismatch fail-stops. This
//! replaces the older unconditional-skip expected-trap vector.

use core::arch::asm;
use riscv::register::mtvec;

use super::Error;
use super::entry::recovery_entry;

/// The record published through `mscratch` during one guarded operation.
///
/// Word layout is the assembly ABI of `recovery_entry`:
/// `expected_mepc`, `cause_mask`, `trapped`, then the recorded
/// `mepc`/`mcause`/`mtval` facts.
#[repr(C)]
pub(crate) struct RecoveryRecord {
    expected_mepc: usize,
    cause_mask: usize,
    trapped: usize,
    info_mepc: usize,
    info_mcause: usize,
    info_mtval: usize,
}

// Bind the hand-written word offsets in `recovery_entry` to this layout.
const WORD: usize = core::mem::size_of::<usize>();
const _: () = assert!(core::mem::offset_of!(RecoveryRecord, expected_mepc) == 0);
const _: () = assert!(core::mem::offset_of!(RecoveryRecord, cause_mask) == WORD);
const _: () = assert!(core::mem::offset_of!(RecoveryRecord, trapped) == WORD * 2);
const _: () = assert!(core::mem::offset_of!(RecoveryRecord, info_mepc) == WORD * 3);
const _: () = assert!(core::mem::offset_of!(RecoveryRecord, info_mcause) == WORD * 4);
const _: () = assert!(core::mem::offset_of!(RecoveryRecord, info_mtval) == WORD * 5);

impl RecoveryRecord {
    /// Number of XLEN words in the record; consumed by `entry`.
    pub(crate) const WORDS: usize = core::mem::size_of::<Self>() / core::mem::size_of::<usize>();

    fn new(cause_mask: usize) -> Self {
        Self {
            expected_mepc: 0,
            cause_mask,
            trapped: 0,
            info_mepc: 0,
            info_mcause: 0,
            info_mtval: 0,
        }
    }

    /// Whether the guarded instruction faulted and was recovered.
    fn trapped(&self) -> bool {
        self.trapped != 0
    }
}

#[cfg(target_pointer_width = "32")]
macro_rules! store_word {
    ($reg:ident => [$base:ident]) => {
        concat!("sw ", stringify!($reg), ", 0(", stringify!($base), ")")
    };
}

#[cfg(target_pointer_width = "64")]
macro_rules! store_word {
    ($reg:ident => [$base:ident]) => {
        concat!("sd ", stringify!($reg), ", 0(", stringify!($base), ")")
    };
}

const MPRV_BIT: usize = 1 << 17;
const MXR_BIT: usize = 1 << 19;

/// Allowed-cause masks for the generated guards.
pub(crate) mod cause {
    /// Illegal instruction (unimplemented CSR).
    pub(crate) const ILLEGAL: usize = 1 << 2;
    /// Load access fault.
    pub(crate) const LOAD_ACCESS: usize = 1 << 5;
    /// Store access fault.
    pub(crate) const STORE_ACCESS: usize = 1 << 7;
    /// Load page fault.
    pub(crate) const LOAD_PAGE: usize = 1 << 13;
    /// Store page fault.
    pub(crate) const STORE_PAGE: usize = 1 << 15;
}

/// Generate one guarded operation: publish the record, install the recovery
/// vector, execute exactly one guarded instruction with `MPRV|MXR` set (so
/// the access uses the trapped context's privilege), retire the record,
/// restore `mtvec`/`mstatus`, and report whether the operation faulted.
///
/// Register contract: `t0` carries the data, `t1` the address, `t3`/`t4`
/// the `mstatus`/`mtvec` scratch, `a3` the record pointer. The guarded
/// instruction is forced to 4 bytes by `.option norvc`, matching the
/// recovery entry's skip.
///
/// The generated functions are `#[inline(never)]`: with inlining the
/// compiler may keep live values in the fixed registers or sink surrounding
/// memory accesses into the MPRV window.
macro_rules! guarded_access {
    ($(#[$attr:meta])* $name:ident, $insn:literal, $cause_mask:expr) => {
        $(#[$attr])*
        #[inline(never)]
        fn $name(addr: usize, data: usize) -> Result<usize, Error> {
            let mut data = data;
            let mut record = RecoveryRecord::new($cause_mask);
            // SAFETY: machine interrupts are disabled for the whole window
            // (M-mode trap handling or boot with `mie` masked), so the
            // temporary `mtvec`/`mscratch` publication cannot be observed
            // by an unrelated trap. All touched CSRs are restored on both
            // the success and the recovered-fault paths.
            unsafe {
                let prev_mtvec = mtvec::read().bits();
                mtvec::write(mtvec::Mtvec::new(
                    recovery_entry as *const () as _,
                    mtvec::TrapMode::Direct,
                ));
                asm!(
                    // Publish the record, then the exact faulting address.
                    "csrw mscratch, a3",
                    "lla t2, 2f",
                    store_word!(t2 => [a3]),
                    "csrrs t3, mstatus, t3",
                    ".option push",
                    ".option norvc",
                    "2:",
                    $insn,
                    ".option pop",
                    "csrw mstatus, t3",
                    "csrw mscratch, zero",
                    "csrw mtvec, t4",
                    in("t1") addr,
                    in("t3") MPRV_BIT | MXR_BIT,
                    in("t4") prev_mtvec,
                    in("a3") &mut record as *mut RecoveryRecord,
                    inout("t0") data,
                    out("t2") _,
                );
            }
            if record.trapped() {
                return Err(fault_of(&record));
            }
            Ok(data)
        }
    };
}

/// Builds the redirect-ready secondary fault from a trapped record.
fn fault_of(record: &RecoveryRecord) -> Error {
    Error::MemoryFault {
        cause: record.info_mcause,
        tval: record.info_mtval,
    }
}

guarded_access!(
    /// Read one byte at `addr` (`lbu`).
    read_u8,
    "lbu t0, 0(t1)",
    cause::LOAD_ACCESS | cause::LOAD_PAGE
);

guarded_access!(
    /// Read one halfword at `addr` (`lhu`); `addr` must be halfword-aligned.
    read_u16,
    "lhu t0, 0(t1)",
    cause::LOAD_ACCESS | cause::LOAD_PAGE
);

guarded_access!(
    /// Write one byte `data` at `addr` (`sb`).
    write_u8,
    "sb t0, 0(t1)",
    cause::STORE_ACCESS | cause::STORE_PAGE
);

/// Read the 12-bit-immediate CSR `CSR` under the recovery guard.
///
/// Reading an unimplemented CSR raises an illegal-instruction exception,
/// which the recovery entry turns into [`Error::UnsupportedInstruction`]. The
/// guard runs with interrupts disabled, exactly like the memory guards.
#[inline(never)]
pub fn read_csr_guarded<const CSR: u16>() -> Result<usize, Error> {
    let mut data: usize = 0;
    let mut record = RecoveryRecord::new(cause::ILLEGAL);
    // SAFETY: same window contract as the generated guards; the CSR number
    // is a compile-time immediate.
    unsafe {
        let prev_mtvec = mtvec::read().bits();
        mtvec::write(mtvec::Mtvec::new(
            recovery_entry as *const () as _,
            mtvec::TrapMode::Direct,
        ));
        asm!(
            "csrw mscratch, a3",
            "lla t2, 2f",
            store_word!(t2 => [a3]),
            "csrrs t3, mstatus, t3",
            ".option push",
            ".option norvc",
            "2:",
            "csrr t0, {csr}",
            ".option pop",
            "csrw mstatus, t3",
            "csrw mscratch, zero",
            "csrw mtvec, t4",
            csr = const CSR,
            in("t3") MPRV_BIT | MXR_BIT,
            in("t4") prev_mtvec,
            in("a3") &mut record as *mut RecoveryRecord,
            inout("t0") data,
            out("t2") _,
        );
    }
    if record.trapped() {
        // Reading the CSR itself raised illegal instruction: the machine
        // cannot supply the counter, so no register or PC commits and the
        // original illegal-instruction exception is redirected (design
        // section 11.3).
        return Err(Error::UnsupportedInstruction);
    }
    Ok(data)
}

/// Write `value` to the 12-bit-immediate CSR `CSR` under the recovery guard.
/// Returns `Err` when the CSR instruction faults.
///
/// Mirrors [`read_csr_guarded`]: an illegal-instruction exception from an
/// unimplemented CSR becomes [`Error::UnsupportedInstruction`].
#[inline(never)]
pub fn write_csr_guarded<const CSR: u16>(value: usize) -> Result<(), Error> {
    let mut record = RecoveryRecord::new(cause::ILLEGAL);
    // SAFETY: same window contract as the other guards.
    unsafe {
        let prev_mtvec = mtvec::read().bits();
        mtvec::write(mtvec::Mtvec::new(
            recovery_entry as *const () as _,
            mtvec::TrapMode::Direct,
        ));
        asm!(
            "csrw mscratch, a3",
            "lla t2, 2f",
            store_word!(t2 => [a3]),
            "csrrs t3, mstatus, t3",
            ".option push",
            ".option norvc",
            "2:",
            "csrw {csr}, t0",
            ".option pop",
            "csrw mstatus, t3",
            "csrw mscratch, zero",
            "csrw mtvec, t4",
            csr = const CSR,
            in("t3") MPRV_BIT | MXR_BIT,
            in("t4") prev_mtvec,
            in("a3") &mut record as *mut RecoveryRecord,
            in("t0") value,
            out("t2") _,
        );
    }
    if record.trapped() {
        return Err(Error::UnsupportedInstruction);
    }
    Ok(())
}

/// Exchanges `value` with the 12-bit-immediate CSR `CSR` under the recovery
/// guard and returns the previous CSR value.
///
/// This is used by feature discovery to test a writable counter without
/// exposing an unguarded CSR instruction to policy code.
#[inline(never)]
pub fn swap_csr_guarded<const CSR: u16>(value: usize) -> Result<usize, Error> {
    let mut data = value;
    let mut record = RecoveryRecord::new(cause::ILLEGAL);
    // SAFETY: same window contract as the other guarded CSR operations.
    unsafe {
        let prev_mtvec = mtvec::read().bits();
        mtvec::write(mtvec::Mtvec::new(
            recovery_entry as *const () as _,
            mtvec::TrapMode::Direct,
        ));
        asm!(
            "csrw mscratch, a3",
            "lla t2, 2f",
            store_word!(t2 => [a3]),
            "csrrs t3, mstatus, t3",
            ".option push",
            ".option norvc",
            "2:",
            "csrrw t0, {csr}, t0",
            ".option pop",
            "csrw mstatus, t3",
            "csrw mscratch, zero",
            "csrw mtvec, t4",
            csr = const CSR,
            in("t3") MPRV_BIT | MXR_BIT,
            in("t4") prev_mtvec,
            in("a3") &mut record as *mut RecoveryRecord,
            inout("t0") data,
            out("t2") _,
        );
    }
    if record.trapped() {
        return Err(Error::UnsupportedInstruction);
    }
    Ok(data)
}

/// Fetch the instruction at `mepc`, returning its encoding and length.
///
/// Only 16- and 32-bit encodings are fetched; longer encodings read as
/// 4 bytes, fail decode, and are redirected.
// TODO: fetch 48-bit-and-longer encodings together with compressed support.
pub(crate) fn fetch(mepc: usize) -> Result<(u32, usize), Error> {
    let low = read_u16(mepc, 0)? as u32;
    if riscv_decode::instruction_length(low as u16) == 2 {
        Ok((low, 2))
    } else {
        let high = read_u16(mepc + 2, 0)? as u32;
        Ok((low | (high << 16), 4))
    }
}

/// Read a `kind`-wide value at `addr`, composing the bytes little-endian.
pub(crate) fn read_value(addr: usize, kind: ValueKind) -> Result<usize, Error> {
    let mut data = 0;
    for i in (0..kind.width()).rev() {
        data = (data << 8) | read_u8(addr + i, 0)?;
    }
    Ok(data)
}

/// Write a `kind`-wide `value` at `addr`, decomposing it little-endian.
pub(crate) fn write_value(addr: usize, value: usize, kind: ValueKind) -> Result<(), Error> {
    for i in 0..kind.width() {
        write_u8(addr + i, (value >> (8 * i)) & 0xff)?;
    }
    Ok(())
}

use super::decode::ValueKind;
