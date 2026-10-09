//! The expected-fault recovery mechanism for guarded machine operations.
//!
//! A guarded operation publishes a [`RecoveryRecord`] through `mscratch`
//! and switches `mtvec` to the private `recovery_entry` for the duration of
//! exactly one guarded instruction. The entry accepts the fault only on an
//! exact record/origin/PC/cause match; every mismatch fail-stops. This
//! replaces the older unconditional-skip expected-trap vector.

#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
use core::arch::asm;
#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
use riscv::register::{mcause, mepc, mtval, mtvec};

use super::Error;
#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
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

    #[allow(unused)]
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
    #[allow(unused)]
    fn trapped(&self) -> bool {
        self.trapped != 0
    }
}

#[cfg(target_arch = "riscv32")]
macro_rules! store_word {
    ($reg:ident => [$base:ident]) => {
        concat!("sw ", stringify!($reg), ", 0(", stringify!($base), ")")
    };
}

#[cfg(target_arch = "riscv64")]
macro_rules! store_word {
    ($reg:ident => [$base:ident]) => {
        concat!("sd ", stringify!($reg), ", 0(", stringify!($base), ")")
    };
}

#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
const MPRV_BIT: usize = 1 << 17;
#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
const MXR_BIT: usize = 1 << 19;

/// Allowed-cause masks for the generated guards.
#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
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
/// Register contract: `t0` carries data, `t1` the address, and `t4` the
/// saved `mtvec`. After publishing the record, `a3` saves `mstatus`;
/// recovery may clobber `t0`–`t3`, but leaves `a3` and `t4` intact. The guarded
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
            match () {
                #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
                () => {
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
                            // The record is in mscratch; a3 survives the recovery vector.
                            "csrrs a3, mstatus, t3",
                            ".option push",
                            ".option norvc",
                            "2:",
                            $insn,
                            ".option pop",
                            "csrw mstatus, a3",
                            "csrw mscratch, zero",
                            "csrw mtvec, t4",
                            inout("t1") addr => _,
                            inout("t3") MPRV_BIT | MXR_BIT => _,
                            in("t4") prev_mtvec,
                            inout("a3") &mut record as *mut RecoveryRecord => _,
                            inout("t0") data,
                            out("t2") _,
                        );
                    }
                    if record.trapped() {
                        return Err(fault_of(&record));
                    }
                    Ok(data)
                }
                #[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
                () => {
                    let _ = (addr, data);
                    unimplemented!("Guarded memory access requires a RISC-V target");
                }
            }
        }
    };
}

/// Builds the redirect-ready secondary fault from a trapped record.
#[allow(unused)]
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

/// Saved enclosing trap facts for a guarded machine instruction.
///
/// Keep this guard inside the machine-interrupt-disabled window so no
/// unrelated trap can replace the captured facts before they are restored.
#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
struct CsrTrapState {
    mepc: usize,
    mcause: usize,
    mtval: usize,
    #[cfg(target_arch = "riscv32")]
    mstatush: Option<usize>,
}

#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
impl CsrTrapState {
    fn capture() -> Self {
        #[cfg(target_arch = "riscv32")]
        let mstatush = if riscv::register::misa::read().has_extension('H') {
            let value;
            // Capture MPV before a nested trap can replace the guest origin.
            // SAFETY:
            // 1. Guarded machine operations run in M-mode.
            // 2. The H check above establishes mstatush on RV32.
            unsafe {
                asm!("csrr {value}, mstatush", value = out(reg) value, options(nomem, nostack));
            }
            Some(value)
        } else {
            None
        };
        Self {
            mepc: mepc::read(),
            mcause: mcause::read().bits(),
            mtval: mtval::read(),
            #[cfg(target_arch = "riscv32")]
            mstatush,
        }
    }
}

#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
impl Drop for CsrTrapState {
    fn drop(&mut self) {
        // SAFETY:
        // 1. The guard retired its record and restored mstatus/mtvec/mscratch.
        // 2. Drop runs inside machine::free, still in M-mode with interrupts masked.
        // 3. These values were captured on this hart before the guarded instruction.
        unsafe {
            asm!(
                "csrw mepc, {mepc}",
                "csrw mcause, {mcause}",
                "csrw mtval, {mtval}",
                mepc = in(reg) self.mepc,
                mcause = in(reg) self.mcause,
                mtval = in(reg) self.mtval,
                options(nomem),
            );
            #[cfg(target_arch = "riscv32")]
            if let Some(value) = self.mstatush {
                asm!("csrw mstatush, {value}", value = in(reg) value, options(nomem, nostack));
            }
        }
    }
}

/// Executes one machine instruction under the same register contract as the
/// memory guards. The saved trap state drops before machine interrupts are
/// restored, including when an unsupported instruction returns an error.
#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
macro_rules! guarded_machine_instruction {
    ($instruction:literal, $value:expr $(, $csr:ident)?) => {
        riscv::interrupt::machine::free(|| {
            let _trap_state = CsrTrapState::capture();
            let mut data: usize = $value;
            let mut record = RecoveryRecord::new(cause::ILLEGAL);
            // SAFETY:
            // 1. Runtime runs in M-mode; machine::free masks interrupts for the window.
            // 2. Any CSR number is a compile-time immediate. The live stack record
            //    has recovery_entry's checked layout; fixed-register clobbers are declared.
            // 3. Assembly restores mstatus/mtvec/mscratch on success or recovery;
            //    CsrTrapState restores enclosing trap facts before interrupts resume.
            unsafe {
                let prev_mtvec = mtvec::read().bits();
                mtvec::write(mtvec::Mtvec::new(
                    recovery_entry as *const () as _,
                    mtvec::TrapMode::Direct,
                ));
                asm!(
                    "csrrw t5, mscratch, a3",
                    "lla t2, 2f",
                    store_word!(t2 => [a3]),
                    // The record is in mscratch; a3 survives the recovery vector.
                    "csrrs a3, mstatus, t3",
                    ".option push",
                    ".option norvc",
                    "2:",
                    $instruction,
                    ".option pop",
                    "csrw mstatus, a3",
                    "csrw mscratch, t5",
                    "csrw mtvec, t4",
                    $(csr = const $csr,)?
                    inout("t3") MPRV_BIT | MXR_BIT => _,
                    in("t4") prev_mtvec,
                    inout("a3") &mut record as *mut RecoveryRecord => _,
                    inout("t0") data,
                    out("t1") _,
                    out("t2") _,
                    out("t5") _,
                );
            }
            if record.trapped() {
                Err(Error::UnsupportedInstruction)
            } else {
                Ok(data)
            }
        })
    };
}

/// Reads the 12-bit-immediate CSR `CSR` under the recovery guard.
///
/// Reading an unimplemented CSR raises an illegal-instruction exception,
/// which the recovery entry turns into [`Error::UnsupportedInstruction`].
/// The guard masks machine interrupts; unexpected faults fail-stop.
#[inline(never)]
pub fn read_csr_guarded<const CSR: u16>() -> Result<usize, Error> {
    match () {
        #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
        () => guarded_machine_instruction!("csrr t0, {csr}", 0, CSR),
        #[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
        () => {
            let _ = CSR;
            unimplemented!("Guarded CSR access requires a RISC-V target");
        }
    }
}

/// Writes `value` to the 12-bit-immediate CSR `CSR` under the recovery guard.
///
/// An illegal-instruction exception becomes [`Error::UnsupportedInstruction`].
/// Unexpected faults fail-stop.
#[inline(never)]
pub fn write_csr_guarded<const CSR: u16>(value: usize) -> Result<(), Error> {
    match () {
        #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
        () => guarded_machine_instruction!("csrw {csr}, t0", value, CSR).map(|_| ()),
        #[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
        () => {
            let _ = (CSR, value);
            unimplemented!("Guarded CSR access requires a RISC-V target");
        }
    }
}

/// Exchanges `value` with the 12-bit-immediate CSR `CSR` under the recovery
/// guard and returns the previous CSR value.
///
/// IMSIC claim uses the exchange to read and acknowledge one interrupt
/// atomically.
#[inline(never)]
pub fn swap_csr_guarded<const CSR: u16>(value: usize) -> Result<usize, Error> {
    match () {
        #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
        () => guarded_machine_instruction!("csrrw t0, {csr}, t0", value, CSR),
        #[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
        () => {
            let _ = (CSR, value);
            unimplemented!("Guarded CSR access requires a RISC-V target");
        }
    }
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

/// Synchronizes address translations under the guarded instruction recovery protocol.
pub(crate) fn sfence_vma_guarded() -> Result<(), Error> {
    match () {
        #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
        () => guarded_machine_instruction!("sfence.vma x0, x0", 0).map(|_| ()),
        #[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
        () => unimplemented!("Guarded address-translation fence requires a RISC-V target"),
    }
}
