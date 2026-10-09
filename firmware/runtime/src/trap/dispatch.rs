//! Private trap classification and dispatch.
//!
//! The entry supplies the saved [`TrapFrame`]. Machine
//! timer/software/external interrupts are Runtime-private transports; other
//! machine interrupt causes are offered to the installed
//! [`MachineInterruptPolicy`](crate::machine_irq::MachineInterruptPolicy), if
//! any; an exception is either an SBI ecall, one of the emulated classes, or
//! is software-redirected to S/HS. Nothing here is public policy surface.

use crate::csr::Mie;
use core::arch::asm;

use riscv::register::{mepc, mstatus, satp, sstatus};
use rustsbi::SbiRet;

use super::Error;
use super::frame::TrapFrame;
use super::redirect::redirect_trap;
use super::{emulate, init};
use crate::boot::NextStage;
use crate::hart::{self, HartEvent};

/// Machine interrupt cause codes.
const MSOFT: usize = 3;
const MTIMER: usize = 7;
const MEXT: usize = 11;

/// Exception cause codes.
const ECALL_FROM_S: usize = 9;
const ILLEGAL_INSTRUCTION: usize = 2;
const LOAD_MISALIGNED: usize = 4;
const STORE_MISALIGNED: usize = 6;
const LOAD_FAULT: usize = 5;
const STORE_FAULT: usize = 7;

/// Dispatches the saved frame supplied by the normal trap entry.
///
/// The live trap CSRs still hold this trap's facts on entry; committed
/// effects (instruction advance, redirection, next-stage entry) write the
/// live CSRs, and the entry's `mret` consumes them.
pub(crate) extern "C" fn trap_dispatch(frame: &mut TrapFrame) {
    let code = frame.mcause;
    if code & (1 << (usize::BITS - 1)) != 0 {
        let interrupt = code & !(1 << (usize::BITS - 1));
        match interrupt {
            MSOFT => machine_soft(frame),
            MTIMER => machine_timer(),
            MEXT => machine_external(frame),
            // Interrupts reach M-mode only when delegation could not move
            // them; there is no lower owner that would receive a redirect.
            // An installed policy may own such a cause (the Smmtt MSDEI, for
            // example); unclaimed causes remain fatal.
            _ => {
                if !crate::machine_irq::get()
                    .is_some_and(|policy| policy.handle_interrupt(interrupt))
                {
                    fatal();
                }
            }
        }
        return;
    }

    // A synchronous exception from M-mode is fatal:
    // only the exact guarded-fault recovery may re-enter, and it never
    // comes through this dispatch.
    if frame.trapped_from_machine() {
        fatal();
    }

    match code {
        ECALL_FROM_S => sbi_ecall(frame),
        ILLEGAL_INSTRUCTION => illegal_instruction(frame),
        LOAD_MISALIGNED => misaligned(frame, Access::Load),
        STORE_MISALIGNED => misaligned(frame, Access::Store),
        LOAD_FAULT => access_fault(frame, Access::Load),
        STORE_FAULT => access_fault(frame, Access::Store),
        // Exceptions requested for delegation but retained by hardware
        // (WARL) still have a correct software-redirection path.
        _ => redirect_or_fatal(None),
    }
}

/// Redirects with the emulation failure's secondary facts, or fail-stops when
/// even the redirect is impossible.
#[inline(never)]
fn redirect_or_fatal(error: Option<Error>) {
    let secondary = match error {
        Some(Error::MemoryFault { cause, tval }) => Some((cause, tval)),
        _ => None,
    };
    if redirect_trap(secondary).is_err() {
        fatal();
    }
}

/// Stops this hart through the stack-independent fail-stop vector.
fn fatal() -> ! {
    // SAFETY:
    // 1. Trap dispatch executes in M-mode, as fail_stop requires.
    // 2. The vector never returns or accesses the retired call chain.
    unsafe {
        core::arch::asm!(
            "tail {fail}",
            fail = sym crate::boot::fail_stop,
            options(noreturn)
        )
    }
}

/// Dispatches an SBI ecall, commits `SbiRet`, and advances past the ecall.
fn sbi_ecall(frame: &mut TrapFrame) {
    let extension = frame.read_x(17); // a7
    let function = frame.read_x(16); // a6
    let param = [
        frame.read_x(10),
        frame.read_x(11),
        frame.read_x(12),
        frame.read_x(13),
        frame.read_x(14),
        frame.read_x(15),
    ];

    let hart = hart::current_hart();
    let ret: SbiRet = init::policy(hart).handle_ecall(extension, function, param);
    frame.write_x(10, ret.error);
    frame.write_x(11, ret.value);

    // SAFETY:
    // 1. The M-mode handler owns this hart's return PC with MIE clear.
    // 2. The dispatched ECALL instruction occupies exactly four bytes.
    unsafe { mepc::write(mepc::read() + 4) };

    // The current hart's committed ticket publishes the caller-selected
    // supervisor resume entry for this ecall return. The address is not
    // validated here; preparing the handoff only writes architectural state.
    if let Some(hart::ControlTransfer::NonRetentiveResume(next_stage)) =
        hart::take_control_transfer(hart)
    {
        // SAFETY:
        // 1. This initialized M-mode handler owns the return state with MIE clear.
        // 2. This hart's committed ticket supplies the foreign supervisor entry;
        //    writing mepc does not construct or dereference a Rust pointer.
        unsafe {
            stage_smode_trap_state();
            mstatus::set_mpp(mstatus::MPP::Supervisor);
            mepc::write(next_stage.start_addr);
        }
        frame.x.fill(0);
        riscv::asm::fence_i();
        frame.write_x(10, hart.as_usize());
        frame.write_x(11, next_stage.opaque);
    }
}

/// Emulates pure `time`/`timeh` reads, or redirects the illegal instruction.
#[inline(never)]
fn illegal_instruction(frame: &mut TrapFrame) {
    if let Some(counters) = crate::events::get() {
        counters.record_illegal_instruction();
    }
    if let Err(error) = emulate::emulate_csr_read(frame) {
        redirect_or_fatal(Some(error));
    }
}

/// Emulates a misaligned integer load or store.
#[inline(never)]
fn misaligned(frame: &mut TrapFrame, access: Access) {
    let result = match access {
        Access::Load => {
            if let Some(counters) = crate::events::get() {
                counters.record_misaligned_load();
            }
            emulate::emulate_load(frame)
        }
        Access::Store => {
            if let Some(counters) = crate::events::get() {
                counters.record_misaligned_store();
            }
            emulate::emulate_store(frame)
        }
    };
    if let Err(error) = result {
        redirect_or_fatal(Some(error));
    }
}

/// Completes Supervisor-origin access faults through the platform dispatcher.
///
/// Register writeback and PC advancement require success. A declined or
/// unsupported access redirects the original fault; a fault while fetching
/// the instruction redirects the secondary fetch fault.
fn access_fault(frame: &mut TrapFrame, access: Access) {
    if let Some(counters) = crate::events::get() {
        match access {
            Access::Load => counters.record_access_load(),
            Access::Store => counters.record_access_store(),
        }
    }
    if !frame.is_trapped_from_supervisor() {
        redirect_or_fatal(None);
        return;
    }
    let result = match access {
        Access::Load => emulate::dispatch_load_fault(frame),
        Access::Store => emulate::dispatch_store_fault(frame),
    };
    if let Err(error) = result {
        redirect_or_fatal(Some(error));
    }
}

enum Access {
    Load,
    Store,
}

/// Receives staged hart starts and pending work through a software IPI.
#[inline(never)]
fn machine_soft(frame: &mut TrapFrame) {
    let ipi = crate::ipi::Ipi::current().expect("BUG: software IPI source unavailable");
    if let Some(event) = ipi
        .receive_software()
        .expect("BUG: could not receive software IPI")
    {
        complete_ipi(frame, &ipi, event);
    }
}

/// Acknowledges MTIP and injects the supervisor timer interrupt when required.
fn machine_timer() {
    if crate::timer::Timer::current()
        .and_then(|timer| timer.on_machine_timer())
        .is_err()
    {
        fatal();
    }
}

/// Receives queued firmware work from the selected IMSIC interrupt file.
#[inline(never)]
fn machine_external(frame: &mut TrapFrame) {
    let ipi = match crate::ipi::Ipi::current() {
        Ok(ipi) => ipi,
        Err(crate::ipi::Error::Unavailable) => return,
        Err(error) => panic!("BUG: invalid external IPI capability: {error}"),
    };
    if let Some(event) = ipi
        .receive_external()
        .expect("BUG: could not receive external IPI")
    {
        complete_ipi(frame, &ipi, event);
    }
}

fn complete_ipi(frame: &mut TrapFrame, ipi: &crate::ipi::Ipi, event: HartEvent) {
    match event {
        HartEvent::Start(next_stage) => enter_next_stage(frame, next_stage),
        HartEvent::Park => {
            ipi.prepare_wait()
                .expect("BUG: IPI capability used on another hart");
            riscv::asm::wfi();
        }
        HartEvent::None => {}
    }
}

/// Prepares the saved frame and live CSRs for a staged next-stage entry.
fn enter_next_stage(frame: &mut TrapFrame, next: NextStage) {
    // SAFETY:
    // 1. This initialized M-mode handler owns the return state with MIE clear.
    // 2. The hart cell supplies the staged entry; its timer and traps are ready.
    unsafe { stage_next_mode(next.start_addr, next.next_mode) };
    frame.x.fill(0);
    riscv::asm::fence_i();
    frame.write_x(10, hart::current_hart().as_usize());
    frame.write_x(11, next.opaque);
}

/// Resets outgoing S-mode interrupt and translation state.
///
/// # Safety
///
/// The caller runs in M-mode with MIE clear and owns this hart's outgoing
/// supervisor state.
unsafe fn stage_smode_trap_state() {
    // SAFETY:
    // 1. The caller retains M-mode with machine interrupts disabled.
    // 2. It owns the outgoing supervisor state being cleared before handoff.
    unsafe {
        asm!("csrw sie, zero", options(nomem));
        sstatus::clear_sie();
        satp::write(satp::Satp::from_bits(0));
    }
}

/// Stages the privilege mode and entry address for the next `mret`.
///
/// The transition clears outgoing supervisor state, restores timer transport,
/// and enables the selected wake source. `MPIE` is set for the return.
///
/// # Safety
///
/// The caller runs in M-mode with MIE clear, owns this hart's return state,
/// and has initialized its traps and timer. The caller selects the foreign
/// next-stage mode and entry. This preparation writes the address to `mepc`
/// without validating it or dereferencing it as a Rust pointer.
pub(crate) unsafe fn stage_next_mode(start_addr: usize, next_mode: mstatus::MPP) {
    // SAFETY:
    // 1. The caller owns this hart's return state in M-mode with MIE clear.
    // 2. The staged handoff supplies the caller-selected mode and foreign PC;
    //    preparing those CSRs does not dereference the target as a Rust pointer.
    // 3. Runtime owns the initialized interrupt controls and timer transport.
    unsafe {
        stage_smode_trap_state();
        mstatus::set_mpie();
        mstatus::set_mpp(next_mode);
        match crate::ipi::Ipi::current() {
            Ok(ipi) => ipi
                .prepare_wait()
                .expect("BUG: IPI capability used on another hart"),
            Err(crate::ipi::Error::Unavailable) => Mie::set_bits(Mie::MACHINE_SOFTWARE),
            Err(error) => panic!("BUG: invalid next-stage IPI capability: {error}"),
        }
        crate::timer::Timer::current()
            .and_then(|timer| timer.prepare_next_stage())
            .expect("BUG: next-stage timer is not initialized");
        mepc::write(start_addr);
    }
}

/// Handles caller-saved time destinations before saving s0-s11.
/// For example, `rdtime a0` writes a caller-saved register and can return here;
/// `rdtime s0` needs the full frame and continues to normal emulation.
///
/// # Safety
///
/// 1. `frame` points to writable, aligned storage for a `TrapFrame`. Entry has
///    initialized its caller-saved register slots and saved CSR fields.
/// 2. The caller handles that trap in M-mode with MIE clear. The s0-s11 slots
///    may be uninitialized; no reference to the complete frame may be formed.
#[inline(never)]
pub(super) unsafe extern "C" fn try_fast_emulate_time(frame: *mut TrapFrame) -> bool {
    // SAFETY:
    // 1. Entry supplies aligned frame storage valid for these raw accesses.
    // 2. It initialized the named CSR fields before this call.
    let (status, cause, inst, pc) = unsafe {
        (
            (*frame).mstatus,
            (*frame).mcause,
            (*frame).mtval as u32,
            (*frame).mepc,
        )
    };
    // mstatus.MPP and the funct3/rs1/opcode fields of a CSRRS register read.
    const MPP_MASK: usize = 0b11 << 11;
    const CSR_READ_MASK: u32 = 0x000f_f07f;
    const CSR_READ_ENCODING: u32 = 0x2073;

    let rd = ((inst >> 7) & 31) as usize;
    if cause != ILLEGAL_INSTRUCTION
        || status & MPP_MASK == MPP_MASK
        || inst & CSR_READ_MASK != CSR_READ_ENCODING
        || !matches!(rd, 0 | 1 | 5..=7 | 10..=17 | 28..=31)
    {
        return false;
    }
    let Some(value) = emulate::device_counter_word((inst >> 20) as u16, status) else {
        return false;
    };
    if let Some(counters) = crate::events::get() {
        counters.record_illegal_instruction();
    }
    if rd != 0 {
        // SAFETY:
        // 1. Entry supplies writable storage for the complete frame allocation.
        // 2. The test above restricts nonzero rd to an initialized caller-saved slot.
        unsafe {
            core::ptr::addr_of_mut!((*frame).x)
                .cast::<usize>()
                .add(rd - 1)
                .write(value);
        }
    }
    // SAFETY:
    // 1. Entry retains this trap in M-mode with MIE clear and a saved return PC.
    // 2. The pure 32-bit CSR read completed without modifying other trap CSRs.
    unsafe {
        mepc::write(pc.wrapping_add(4));
    }
    true
}
