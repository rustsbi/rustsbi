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

/// Dispatches a load access fault to the installed platform handler.
///
/// Fetches and decodes the instruction at `mepc`, translates the faulting
/// S-mode virtual address to a physical address, forwards the physical
/// address and access width to the handler, writes the extended value back
/// to `rd`, and advances `mepc`.
///
/// Reports [`Error::UnsupportedInstruction`] when no handler is installed,
/// the address translation mode is unsupported, or the handler declines the
/// address. The original access fault is preserved for the caller to redirect.
pub fn dispatch_access_load(frame: &mut TrapFrame) -> Result<(), Error> {
    reject_machine_origin(frame)?;
    let handlers = crate::access::get().ok_or(Error::UnsupportedInstruction)?;
    with_trap_facts(|facts| {
        let (raw, len) = fetch(facts.mepc)?;
        let op = decode::decode_load(raw)?;
        let phys = translate_access_address(facts.mtval)?;
        let raw_value =
            (handlers.load)(phys, op.kind.width()).ok_or(Error::UnsupportedInstruction)?;
        frame.write_x(op.rd as usize, op.kind.extend(raw_value));
        Ok(len)
    })
}

/// Dispatches a store access fault to the installed platform handler.
///
/// Fetches and decodes the instruction at `mepc`, reads the `rs2` value,
/// translates the faulting S-mode virtual address to a physical address,
/// and forwards the physical address, width, and value to the handler,
/// then advances `mepc`.
///
/// Reports [`Error::UnsupportedInstruction`] when no handler is installed,
/// the address translation mode is unsupported, or the handler declines the
/// address. The original access fault is preserved for the caller to redirect.
pub fn dispatch_access_store(frame: &mut TrapFrame) -> Result<(), Error> {
    reject_machine_origin(frame)?;
    let handlers = crate::access::get().ok_or(Error::UnsupportedInstruction)?;
    with_trap_facts(|facts| {
        let (raw, len) = fetch(facts.mepc)?;
        let op = decode::decode_store(raw)?;
        let value = frame.read_x(op.rs2 as usize);
        let phys = translate_access_address(facts.mtval)?;
        if !(handlers.store)(phys, op.kind.width(), value) {
            return Err(Error::UnsupportedInstruction);
        }
        Ok(len)
    })
}

/// Translates the faulting S-mode virtual address to a physical address.
///
/// Bare mode: the virtual address is already physical. Sv39: walked through
/// physical page-table reads (M-mode loads always bypass satp). Any other
/// mode, or a failed walk, declines the access.
fn translate_access_address(va: usize) -> Result<usize, Error> {
    #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
    {
        let satp_val: usize;
        // SAFETY: csrr satp is a pure read; only reachable from M-mode trap handling.
        unsafe { asm!("csrr {}, satp", out(reg) satp_val, options(nomem, nostack)) };

        #[cfg(target_arch = "riscv64")]
        {
            // RV64 satp: mode [63:60], ASID [59:44], PPN [43:0].
            const BARE: usize = 0;
            const SV39: usize = 8;
            let mode = satp_val >> 60;
            let root = (satp_val & ((1usize << 44) - 1)) << 12;
            match mode {
                BARE => Ok(va),
                SV39 => sv39_translate(root, va).ok_or(Error::UnsupportedInstruction),
                _ => Err(Error::UnsupportedInstruction),
            }
        }
        #[cfg(target_arch = "riscv32")]
        {
            // RV32 satp: mode [31], ASID [30:22], PPN [21:0].
            if satp_val >> 31 == 0 {
                return Ok(va);
            }
            return Err(Error::UnsupportedInstruction);
        }
    }
    // Non-RISC-V targets (host unit-test builds) have no satp semantics.
    #[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
    Err(Error::UnsupportedInstruction)
}

/// Walks the Sv39 page table rooted at `root` to translate `va`.
///
/// Page-table entries are read with raw M-mode loads, which always use
/// physical addresses regardless of satp.
///
/// # Limitations
///
/// A fault during a PTE read is not caught by the existing recovery guard.
/// Such a fault would re-enter the machine trap handler; the caller must
/// accept that risk for now. Future work may add a guarded PTE read.
///
/// Returns `None` when the entry chain is invalid or the leaf has a
/// reserved encoding (W without R).
#[cfg(target_arch = "riscv64")]
fn sv39_translate(root: usize, va: usize) -> Option<usize> {
    // SATP64: the PPN field is 44 bits; mask out the high bits of a PTE's
    // PPN field (bits [53:10] → 44 bits after shifting) to avoid carrying
    // reserved PTE bits into the physical address.
    const PPN_MASK: usize = (1 << 44) - 1;
    const PTE_V: u64 = 1 << 0;
    const PTE_R: u64 = 1 << 1;
    const PTE_W: u64 = 1 << 2;
    const PTE_X: u64 = 1 << 3;

    let offset = va & 0xfff;
    let vpn = [(va >> 12) & 0x1ff, (va >> 21) & 0x1ff, (va >> 30) & 0x1ff];

    let mut table = root;
    for level in (0..3usize).rev() {
        let pte_addr = table + vpn[level] * core::mem::size_of::<u64>();
        // SAFETY: M-mode loads bypass satp and are always physical on RISC-V.
        let pte = unsafe { core::ptr::read_volatile(pte_addr as *const u64) };
        if pte & PTE_V == 0 || (pte & PTE_W != 0 && pte & PTE_R == 0) {
            return None;
        }
        let ppn = ((pte >> 10) as usize) & PPN_MASK;
        if pte & (PTE_R | PTE_X) != 0 {
            return Some(match level {
                0 => (ppn << 12) | offset,
                1 => ((ppn & !0x1ff) << 12) | (va & ((1 << 21) - 1)),
                2 => ((ppn & !0x3_ffff) << 12) | (va & ((1 << 30) - 1)),
                _ => return None,
            });
        }
        table = ppn << 12;
    }
    None
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
    if raw & 0x000f_f07f == 0x2073
        && let Some(value) = device_counter_word((raw >> 20) as u16)
    {
        frame.write_x(((raw >> 7) & 31) as usize, value);
        // SAFETY: one completed 32-bit pure CSR read from lower privilege.
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
        let value = counter_word(op.csr)?;
        frame.write_x(op.rd as usize, value);
        Ok(len)
    })
}
