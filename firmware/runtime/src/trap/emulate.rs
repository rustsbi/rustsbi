//! Complete trapped-instruction operations: the orchestration of fetch,
//! decode, memory access, register write-back, and `mepc` advance as one
//! function (OpenSBI `sbi_trap_ldst.c` and `sbi_emulate_csr.c`).
//!
//! # Contract
//!
//! Every operation must be called while handling a trap, before any `mepc`
//! advance: inputs bind to the hardware trap CSRs, and the operations never
//! advance `mepc` unless the whole instruction completed.

use core::arch::asm;
use riscv::register::{mcause, mepc, mstatus, mtval};

use super::Error;
use super::decode;
use super::frame::TrapFrame;
use super::recovery;

/// Exception cause codes for secondary-fault remapping.
mod cause {
    /// Instruction access fault.
    pub(crate) const INSTRUCTION_ACCESS: usize = 1;
    /// Load access fault.
    pub(crate) const LOAD_ACCESS: usize = 5;
    /// Instruction page fault.
    pub(crate) const INSTRUCTION_PAGE: usize = 12;
    /// Load page fault.
    pub(crate) const LOAD_PAGE: usize = 13;
}

/// Maps a faulting guarded instruction fetch to the exception the failed
/// fetch must deliver: a load fault on instruction bytes reports as an
/// instruction access or page fault with the failed byte address (design
/// section 11.2).
fn map_fetch_fault(error: Error) -> Error {
    match error {
        Error::MemoryFault { cause, tval } => Error::MemoryFault {
            cause: match cause {
                cause::LOAD_ACCESS => cause::INSTRUCTION_ACCESS,
                cause::LOAD_PAGE => cause::INSTRUCTION_PAGE,
                other => other,
            },
            tval,
        },
        other => other,
    }
}

/// Fetches the instruction at `mepc` with fetch-fault cause mapping.
fn fetch(mepc: usize) -> Result<(u32, usize), Error> {
    recovery::fetch(mepc).map_err(map_fetch_fault)
}

/// The `time` and `timeh` CSR numbers.
const CSR_TIME: u16 = 0xC01;
#[cfg(target_arch = "riscv32")]
const CSR_TIMEH: u16 = 0xC81;

/// The trap facts as they were when the operation started. A faulting
/// guarded access re-traps and clobbers `mepc`/`mcause`/`mtval`/`mstatus`,
/// so operations capture the originals and restore them on every return
/// path — a redirect after a failed emulation must see the original trap.
struct TrapFacts {
    mepc: usize,
    mcause: usize,
    mtval: usize,
    mstatus: usize,
}

impl TrapFacts {
    fn capture() -> Self {
        Self {
            mepc: mepc::read(),
            mcause: mcause::read().bits(),
            mtval: mtval::read(),
            mstatus: mstatus::read().bits(),
        }
    }

    /// Restore the trap CSRs (but not `mepc`) to the captured state.
    ///
    /// # Safety
    ///
    /// M-mode writes to this hart's trap CSRs, with values previously read
    /// from the same registers.
    unsafe fn restore(&self) {
        // SAFETY: M-mode writes to this hart's trap CSRs with values
        // previously read from the same registers.
        unsafe {
            asm!(
                "csrw 0x342, {mcause}",   // mcause
                "csrw 0x343, {mtval}",     // mtval
                mcause = in(reg) self.mcause,
                mtval = in(reg) self.mtval,
                options(nomem),
            );
            mstatus::write(mstatus::Mstatus::from_bits(self.mstatus));
        }
    }

    /// Restore everything and advance `mepc` by `len` (successful path).
    unsafe fn restore_and_advance(&self, len: usize) {
        // SAFETY: see `restore`; the mepc advance skips the emulated
        // instruction.
        unsafe {
            self.restore();
            mepc::write(self.mepc + len);
        }
    }

    /// Restore everything including `mepc` (failed path).
    unsafe fn restore_all(&self) {
        // SAFETY: see `restore`; mepc is rewritten with its captured value.
        unsafe {
            self.restore();
            mepc::write(self.mepc);
        }
    }
}

/// Refuse traps that originated from M-mode: emulating them would perform
/// accesses at bare M privilege, and there is no lower-privilege owner to
/// receive a redirect.
fn reject_machine_origin(frame: &TrapFrame) -> Result<(), Error> {
    if frame.trapped_from_machine() {
        Err(Error::MachineOrigin)
    } else {
        Ok(())
    }
}

/// Run `body` with the trap facts captured up front; restore the trap CSRs
/// on every return path, advancing `mepc` by the instruction length (which
/// `body` returns) only when the whole emulation succeeded.
fn with_trap_facts(body: impl FnOnce(&TrapFacts) -> Result<usize, Error>) -> Result<(), Error> {
    let facts = TrapFacts::capture();
    match body(&facts) {
        Ok(len) => {
            // SAFETY: restores the captured trap CSRs and advances mepc
            // past the fully emulated instruction.
            unsafe { facts.restore_and_advance(len) };
            Ok(())
        }
        Err(e) => {
            // SAFETY: restores the captured trap CSRs untouched.
            unsafe { facts.restore_all() };
            Err(e)
        }
    }
}

/// Emulate the trapped misaligned load: fetch and decode the instruction at
/// `mepc`, read `kind`-wide at `mtval` under the trapped context's
/// privilege, write the extended value back to `rd`, and advance `mepc`.
pub fn emulate_load(frame: &mut TrapFrame) -> Result<(), Error> {
    reject_machine_origin(frame)?;
    with_trap_facts(|facts| {
        let (raw, len) = fetch(facts.mepc)?;
        let op = decode::decode_load(raw)?;
        let raw_value = recovery::read_value(facts.mtval, op.kind)?;
        frame.write_x(op.rd as usize, op.kind.extend(raw_value));
        Ok(len)
    })
}

/// Emulate the trapped misaligned store: fetch and decode the instruction at
/// `mepc`, read the value of `rs2` from the trapped context, write it
/// `kind`-wide at `mtval` under the trapped context's privilege, and
/// advance `mepc`.
pub fn emulate_store(frame: &mut TrapFrame) -> Result<(), Error> {
    reject_machine_origin(frame)?;
    with_trap_facts(|facts| {
        let (raw, len) = fetch(facts.mepc)?;
        let op = decode::decode_store(raw)?;
        let value = frame.read_x(op.rs2 as usize);
        recovery::write_value(facts.mtval, value, op.kind)?;
        Ok(len)
    })
}

/// Reads a counter word directly when the platform supplies MMIO time.
pub(super) fn device_counter_word(csr: u16) -> Option<usize> {
    let timer = crate::timer::get()?;
    match csr {
        CSR_TIME => timer.read_time_low(),
        #[cfg(target_pointer_width = "32")]
        CSR_TIMEH => timer.read_time_high(),
        _ => None,
    }
}

/// Reads the requested counter word (`time`, or `timeh` on RV32).
///
/// A direct device counter is used when provided; otherwise the architecture
/// CSR is tried under the recovery guard;
/// only when the hart lacks the counter does the emulation consult the
/// injected platform time source (a memory-mapped `mtime`). If neither
/// exists, the caller redirects the original illegal instruction (design
/// section 11.3, revised 2026-09-06).
fn counter_word(csr: u16) -> Result<usize, Error> {
    if let Some(value) = device_counter_word(csr) {
        return Ok(value);
    }
    match csr {
        CSR_TIME => match recovery::read_csr_guarded::<CSR_TIME>() {
            Ok(value) => Ok(value),
            Err(_) => machine_time()
                .map(|time| time as usize)
                .ok_or(Error::UnsupportedInstruction),
        },
        #[cfg(target_arch = "riscv32")]
        CSR_TIMEH => match recovery::read_csr_guarded::<CSR_TIMEH>() {
            Ok(value) => Ok(value),
            Err(_) => machine_time()
                .map(|time| (time >> 32) as usize)
                .ok_or(Error::UnsupportedInstruction),
        },
        _ => Err(Error::UnsupportedInstruction),
    }
}

/// Reads the current hart's optional device time source.
fn machine_time() -> Option<u64> {
    crate::timer::get().and_then(|timer| timer.read_time())
}

/// Emulate a trapped CSR-read instruction (`csrrs rd, csr, x0`): fetch and
/// decode the instruction at `mepc`, obtain the value from the architecture
/// counter or the explicit device time source, write it back to
/// `rd`, and advance `mepc`.
pub fn emulate_csr_read(frame: &mut TrapFrame) -> Result<(), Error> {
    reject_machine_origin(frame)?;
    let raw = frame.mtval as u32;
    if raw & 0x000f_f07f == 0x2073 {
        if let Some(value) = device_counter_word((raw >> 20) as u16) {
            frame.write_x(((raw >> 7) & 31) as usize, value);
            // SAFETY: one completed 32-bit pure CSR read from lower privilege.
            unsafe { mepc::write(frame.mepc.wrapping_add(4)) };
            return Ok(());
        }
    }
    with_trap_facts(|facts| {
        // Supported CSR reads are 32-bit; mtval may supply their encoding.
        let (raw, len) = if raw & 3 == 3 {
            (raw, 4)
        } else {
            fetch(facts.mepc)?
        };
        let op = decode::decode_csr_read(raw)?;
        let value = counter_word(op.csr)?;
        frame.write_x(op.rd as usize, value);
        Ok(len)
    })
}
