//! Trapped integer access and time-counter emulation.
//!
//! The operations follow the fetch/emulate/redirect structure of OpenSBI's
//! `sbi_trap_ldst.c` and `sbi_emulate_csr.c`.
//!
//! # Contract
//!
//! Every operation must be called while handling a trap, before any `mepc`
//! advance: inputs bind to the hardware trap CSRs, and the operations never
//! advance `mepc` unless the whole instruction completed.

use core::arch::asm;
use riscv::register::{mcause, mepc, mtval};

use crate::csr::{Csr, Csr64, Hcounteren, Htimedelta, Mcounteren, Readable, Scounteren, Time};
#[cfg(target_pointer_width = "32")]
use crate::csr::{Misa, Mstatush, TimeHigh};

use super::Error;
use super::decode;
use super::frame::TrapFrame;
use super::init;
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
/// instruction access or page fault with the failed byte address.
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
const CSR_TIME: u16 = Time::NUMBER;
#[cfg(target_pointer_width = "32")]
const CSR_TIMEH: u16 = TimeHigh::NUMBER;

const TIME_ENABLE: usize = 1 << 1;
const MPP_MASK: usize = 0b11 << 11;
const MPP_SUPERVISOR: usize = 0b01 << 11;

/// The already-authorized origin of one trapped time read.
#[derive(Clone, Copy, Eq, PartialEq)]
enum TimeReadContext {
    Host,
    Guest,
}

/// Authorizes a time read using the original trap's privilege and counter
/// enables. Capture this before any guarded access: on RV32, a nested trap
/// would overwrite `mstatush.MPV`, which is outside the saved XLEN frame.
fn time_read_context(saved_status: usize) -> Result<TimeReadContext, crate::trap::Error> {
    let privilege = saved_status & MPP_MASK;
    if !matches!(privilege, 0 | MPP_SUPERVISOR) {
        return Err(crate::trap::Error::UnsupportedInstruction);
    }
    #[cfg(target_pointer_width = "64")]
    let guest = saved_status & (1 << 39) != 0;
    #[cfg(target_pointer_width = "32")]
    let guest = if Misa::read()?.has_extension('H') {
        // RV32 with H implements mstatush. Preserve this original MPV before
        // any access enters the recovery path.
        let status_high = Mstatush::read()?;
        status_high & (1 << 7) != 0
    } else {
        false
    };
    if Mcounteren::read_native() & TIME_ENABLE == 0
        || (privilege == 0 && Scounteren::read()? & TIME_ENABLE == 0)
    {
        return Err(crate::trap::Error::UnsupportedInstruction);
    }
    if guest {
        // MPV identifies an H guest trap, so hcounteren is implemented.
        let counter_enable = Hcounteren::read()?;
        if counter_enable & TIME_ENABLE == 0 {
            return Err(crate::trap::Error::UnsupportedInstruction);
        }
        Ok(TimeReadContext::Guest)
    } else {
        Ok(TimeReadContext::Host)
    }
}

/// The trap facts as they were when the operation started. A faulting
/// guarded access re-traps and clobbers `mepc`/`mcause`/`mtval`/`mstatus`,
/// so operations capture the originals and restore them on every return
/// path — a redirect after a failed emulation must see the original trap.
struct TrapFacts {
    mepc: usize,
    mcause: usize,
    mtval: usize,
    mstatus: usize,
    #[cfg(target_pointer_width = "32")]
    mstatush: Option<usize>,
}

impl TrapFacts {
    fn capture() -> Self {
        let saved_status;
        // SAFETY: M-mode may read mstatus. Preserve its complete value because
        // the riscv crate's typed mask excludes the H extension's MPV/GVA bits.
        unsafe {
            asm!("csrr {value}, mstatus", value = out(reg) saved_status, options(nomem, nostack));
        }
        #[cfg(target_pointer_width = "32")]
        let mstatush = if Misa::read()
            .expect("BUG: current machine ISA register unavailable")
            .has_extension('H')
        {
            let value;
            // Capture the guest origin before recovery can overwrite it.
            // SAFETY:
            // 1. The trap handler executes in M-mode.
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
            mstatus: saved_status,
            #[cfg(target_pointer_width = "32")]
            mstatush,
        }
    }

    /// Restores the captured trap CSRs except `mepc`.
    ///
    /// # Safety
    ///
    /// The caller handles this captured trap on the same hart in M-mode with
    /// machine interrupts disabled. No intervening owner may replace its state.
    unsafe fn restore(&self) {
        // SAFETY:
        // 1. The caller retains this trap in M-mode with machine interrupts disabled.
        // 2. These complete values were captured from the same hart's registers.
        unsafe {
            asm!(
                "csrw 0x342, {mcause}",   // mcause
                "csrw 0x343, {mtval}",     // mtval
                "csrw mstatus, {mstatus}",
                mcause = in(reg) self.mcause,
                mtval = in(reg) self.mtval,
                mstatus = in(reg) self.mstatus,
                options(nomem),
            );
            #[cfg(target_pointer_width = "32")]
            if let Some(value) = self.mstatush {
                asm!("csrw mstatush, {value}", value = in(reg) value, options(nomem, nostack));
            }
        }
    }

    /// Restores trap facts and advances `mepc` after successful emulation.
    ///
    /// # Safety
    ///
    /// The requirements of [`Self::restore`] apply. `len` is the completed
    /// instruction's length.
    unsafe fn restore_and_advance(&self, len: usize) {
        // SAFETY:
        // 1. The caller satisfies restore's same-hart M-mode trap contract.
        // 2. len covers the fully emulated instruction, as required by this method.
        unsafe {
            self.restore();
            mepc::write(self.mepc + len);
        }
    }

    /// Restores trap facts including `mepc` after failed emulation.
    ///
    /// # Safety
    ///
    /// The requirements of [`Self::restore`] apply.
    unsafe fn restore_all(&self) {
        // SAFETY: the caller retains restore's same-hart M-mode trap contract;
        // mepc is restored from that same capture.
        unsafe {
            self.restore();
            mepc::write(self.mepc);
        }
    }
}

/// Rejects traps that originated from M-mode: emulating them would perform
/// accesses at bare M privilege, and there is no lower-privilege owner to
/// receive a redirect.
fn reject_machine_origin(frame: &TrapFrame) -> Result<(), Error> {
    if frame.trapped_from_machine() {
        Err(Error::MachineOrigin)
    } else {
        Ok(())
    }
}

/// Runs `body` with the trap facts captured up front; restore the trap CSRs
/// on every return path, advancing `mepc` by the instruction length (which
/// `body` returns) only when the whole emulation succeeded.
fn with_trap_facts(body: impl FnOnce(&TrapFacts) -> Result<usize, Error>) -> Result<(), Error> {
    let facts = TrapFacts::capture();
    match body(&facts) {
        Ok(len) => {
            // SAFETY:
            // 1. The M-mode handler retains this hart's trap with interrupts disabled.
            // 2. body returned the length only after fully completing the instruction.
            unsafe { facts.restore_and_advance(len) };
            Ok(())
        }
        Err(e) => {
            // SAFETY: the M-mode handler still owns the same captured trap
            // with machine interrupts disabled.
            unsafe { facts.restore_all() };
            Err(e)
        }
    }
}

/// Emulates a misaligned integer load under the trapped context's privilege.
///
/// The destination register and `mepc` change only after all bytes are read.
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

/// Emulates a misaligned integer store under the trapped context's privilege.
///
/// A later byte fault can leave earlier writes visible. `mepc` advances only
/// after the complete store succeeds.
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

/// Handles a load access fault via the installed [`AccessDispatcher`](super::AccessDispatcher).
///
/// The dispatcher performs the access itself, so unlike [`emulate_load`] it
/// avoids touching memory at the trapped context's privilege. `mtval` is
/// passed through as the hardware-reported faulting address, uninterpreted.
///
/// # Errors
///
/// Returns [`Error::UnsupportedInstruction`] if no dispatcher is installed,
/// the instruction is not a decodable integer load, or the dispatcher cannot
/// complete the access; the caller then redirects the original fault.
/// M-mode origins return [`Error::MachineOrigin`]. Instruction-fetch faults
/// retain their mapped cause and address in [`Error::MemoryFault`].
pub fn dispatch_load_fault(frame: &mut TrapFrame) -> Result<(), Error> {
    reject_machine_origin(frame)?;
    let dispatcher = init::access_dispatcher().ok_or(Error::UnsupportedInstruction)?;
    with_trap_facts(|facts| {
        let (raw, len) = fetch(facts.mepc)?;
        let op = decode::decode_load(raw)?;
        let raw_value = dispatcher.load(facts.mtval, op.kind)?;
        frame.write_x(op.rd as usize, op.kind.extend(raw_value));
        Ok(len)
    })
}

/// Handles a store access fault via the installed [`AccessDispatcher`](super::AccessDispatcher).
///
/// The dispatcher receives the register value and [`ValueKind`](super::ValueKind),
/// and performs the store with the width specified by that kind.
///
/// # Errors
///
/// Returns [`Error::UnsupportedInstruction`] if no dispatcher is installed,
/// the instruction is not a decodable integer store, or the dispatcher cannot
/// complete the access; the caller then redirects the original fault.
/// M-mode origins return [`Error::MachineOrigin`]. Instruction-fetch faults
/// retain their mapped cause and address in [`Error::MemoryFault`].
pub fn dispatch_store_fault(frame: &mut TrapFrame) -> Result<(), Error> {
    reject_machine_origin(frame)?;
    let dispatcher = init::access_dispatcher().ok_or(Error::UnsupportedInstruction)?;
    with_trap_facts(|facts| {
        let (raw, len) = fetch(facts.mepc)?;
        let op = decode::decode_store(raw)?;
        let value = frame.read_x(op.rs2 as usize);
        dispatcher.store(facts.mtval, op.kind, value)?;
        Ok(len)
    })
}

/// Reads a counter word directly when the platform supplies MMIO time.
pub(super) fn device_counter_word(csr: u16, saved_status: usize) -> Option<usize> {
    if time_read_context(saved_status).ok()? != TimeReadContext::Host {
        // Guest reads need a complete 64-bit time plus htimedelta. Keep
        // their optional guarded accesses on the full trap-facts path.
        return None;
    }
    let time_source = crate::timer::Timer::current()
        .ok()?
        .platform_time_source()?;
    match csr {
        CSR_TIME => Some(time_source.read_time_low()),
        #[cfg(target_pointer_width = "32")]
        CSR_TIMEH => Some(time_source.read_time_high()),
        _ => None,
    }
}

/// Reads the requested counter word (`time`, or `timeh` on RV32).
///
/// Uses the injected MMIO time source when present, otherwise a guarded
/// architectural read. Guest reads apply htimedelta to the complete value
/// before splitting RV32 words. Missing facilities reject the emulation so
/// the caller redirects the original illegal instruction.
fn counter_word(csr: u16, context: TimeReadContext) -> Result<usize, Error> {
    let high_word = match csr {
        CSR_TIME => false,
        #[cfg(target_arch = "riscv32")]
        CSR_TIMEH => true,
        _ => return Err(Error::UnsupportedInstruction),
    };
    let mut value = crate::timer::Timer::current()
        .ok()
        .and_then(|timer| timer.platform_time_source())
        .map(|time_source| time_source.read_time())
        .map_or_else(Time::read64, Ok)
        .map_err(|_| Error::UnsupportedInstruction)?;
    if context == TimeReadContext::Guest {
        let delta = Htimedelta::read64().map_err(|_| Error::UnsupportedInstruction)?;
        value = value.wrapping_add(delta);
    }
    Ok(if high_word {
        (value >> 32) as usize
    } else {
        value as usize
    })
}

/// Emulates a trapped CSR-read instruction (`csrrs rd, csr, x0`): fetches and
/// decode the instruction at `mepc`, obtain the value from the architecture
/// counter or the explicit device time source, write it back to
/// `rd`, and advance `mepc`.
pub fn emulate_csr_read(frame: &mut TrapFrame) -> Result<(), Error> {
    reject_machine_origin(frame)?;
    let context = time_read_context(frame.mstatus)?;
    let raw = frame.mtval as u32;
    if raw & 0x000f_f07f == 0x2073
        && let Some(value) = device_counter_word((raw >> 20) as u16, frame.mstatus)
    {
        frame.write_x(((raw >> 7) & 31) as usize, value);
        // SAFETY:
        // 1. The handler owns this lower-origin trap in M-mode with MIE clear.
        // 2. The decoded pure CSR read completed; its instruction occupies four bytes.
        unsafe { mepc::write(frame.mepc.wrapping_add(4)) };
        return Ok(());
    }
    with_trap_facts(|facts| {
        // Supported CSR reads are 32-bit; mtval may supply their encoding.
        let (raw, len) = if raw & 3 == 3 {
            (raw, 4)
        } else {
            fetch(facts.mepc)?
        };
        let op = decode::decode_csr_read(raw)?;
        let value = counter_word(op.csr, context)?;
        frame.write_x(op.rd as usize, value);
        Ok(len)
    })
}
