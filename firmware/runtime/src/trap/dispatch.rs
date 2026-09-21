//! Private trap classification and dispatch.
//!
//! The entry hands this function the one real [`TrapFrame`]. Machine
//! timer/software/external interrupts are Runtime-private transports; other
//! machine interrupt causes are offered to the installed
//! [`MachineInterruptPolicy`](crate::machine_irq::MachineInterruptPolicy), if
//! any; an exception is either an SBI ecall, one of the emulated classes, or
//! is software-redirected to S/HS. Nothing here is public policy surface.

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

/// The Rust trap dispatch, called by the normal entry with the saved frame.
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

/// Redirect with the emulation failure's secondary facts, or fail-stop when
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

/// The fatal path: diverge into the stack-independent fail-stop vector. Never returns.
fn fatal() -> ! {
    // SAFETY: a diverging tail jump into the naked fail-stop vector; the
    // vector never returns and touches no stack.
    unsafe {
        core::arch::asm!(
            "tail {fail}",
            fail = sym crate::boot::fail_stop,
            options(noreturn)
        )
    }
}

/// The SBI ecall path: extract the standard registers, call the original
/// `RustSBI` policy, commit `SbiRet`, and advance `mepc` by exactly 4.
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

    let ret: SbiRet = init::policy().handle_ecall(extension, function, param);
    frame.write_x(10, ret.error);
    frame.write_x(11, ret.value);

    // SAFETY: M-mode advance of this hart's mepc past the ecall; the ECALL
    // instruction is exactly 4 bytes.
    unsafe { mepc::write(mepc::read() + 4) };

    // A successful non-retentive resume stages the lower-privilege handoff
    // through Runtime's protocol-free marker. The SBI adapter has already
    // validated the operation; dispatch only performs the machine ceremony.
    if let Some(hart::ControlTransfer::NonRetentiveResume(next_stage)) =
        hart::take_control_transfer()
    {
        // SAFETY: M-mode writes to this hart's S-mode and trap CSRs for the
        // staged resume.
        unsafe {
            stage_smode_trap_state();
            mstatus::set_mpp(mstatus::MPP::Supervisor);
            mepc::write(next_stage.start_addr);
        }
        frame.x.fill(0);
        riscv::asm::fence_i();
        frame.write_x(10, hart::current_hart().as_usize());
        frame.write_x(11, next_stage.opaque);
    }
}

/// Illegal-instruction handling for pure `time`/`timeh` reads (design
/// section 11): decode the instruction, obtain the counter through the
/// guarded architecture read, and commit or redirect.
#[inline(never)]
fn illegal_instruction(frame: &mut TrapFrame) {
    if let Some(counters) = crate::events::get() {
        counters.record_illegal_instruction();
    }
    if let Err(error) = emulate::emulate_csr_read(frame) {
        redirect_or_fatal(Some(error));
    }
}

/// Misaligned integer load/store emulation.
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

/// Complete load/store access faults via the installed platform dispatcher.
///
/// The dispatcher first tries to complete the access, but only for Supervisor-
/// origin faults. If it declines, is absent, or the instruction is not a decodable
/// integer load/store, nothing commits and the original fault is redirected.
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

/// The machine software interrupt transport: a staged hart start performs
/// the next-stage entry; otherwise the pending SBI software interrupt (and
/// any queued remote-fence work) is delivered.
#[inline(never)]
fn machine_soft(frame: &mut TrapFrame) {
    let ipi = crate::ipi::get().expect("BUG: software interrupt without an IPI device");
    ipi.clear_current()
        .expect("BUG: IPI backend could not clear the current hart");
    let event = hart::take_local_event();
    crate::ipi::handler()
        .expect("BUG: IPI handler not published")
        .deliver_current();
    match event {
        HartEvent::Start(next_stage) => enter_next_stage(frame, next_stage),
        HartEvent::Park => {
            crate::csr::mie::set_machine_software();
            riscv::asm::wfi();
        }
        HartEvent::None => {}
    }
}

/// The machine timer transport: stop re-trapping and inject the supervisor
/// timer interrupt when the platform lacks Sstc.
fn machine_timer() {
    crate::csr::mie::clear_machine_timer();
    if !init::has_sstc() {
        if let Some(timer) = crate::timer::get() {
            timer.acknowledge_current();
        }
        crate::csr::mip::set_supervisor_timer();
    }
}

/// The machine external interrupt transport: the platform's IPI identity
/// carries the same delivery work as the machine software interrupt.
#[inline(never)]
fn machine_external(frame: &mut TrapFrame) {
    let Some(controller) = crate::irq::get() else {
        return;
    };
    if controller.claim_ipi() {
        let event = hart::take_local_event();
        crate::ipi::handler()
            .expect("BUG: IPI handler not published")
            .deliver_current();
        match event {
            HartEvent::Start(next_stage) => {
                enter_next_stage(frame, next_stage);
            }
            HartEvent::Park => {
                crate::csr::mie::set_machine_software();
                crate::csr::mie::set_machine_external();
                riscv::asm::wfi();
            }
            HartEvent::None => {}
        }
    }
}

/// Perform a staged transition into S/HS mode: program the S-mode entry
/// ceremony on the frame and live CSRs so the entry's restore path lands in
/// the next stage.
fn enter_next_stage(frame: &mut TrapFrame, next: NextStage) {
    // SAFETY: M-mode writes to this hart's S-mode and trap CSRs for the
    // staged entry.
    unsafe { stage_next_mode(next.start_addr, next.next_mode) };
    frame.x.fill(0);
    riscv::asm::fence_i();
    frame.write_x(10, hart::current_hart().as_usize());
    frame.write_x(11, next.opaque);
}

/// Stage the S-mode trap state reset shared by every next-stage entry.
///
/// # Safety
///
/// M-mode writes to this hart's S-mode CSRs.
unsafe fn stage_smode_trap_state() {
    unsafe {
        asm!("csrw sie, zero", options(nomem));
        sstatus::clear_sie();
        satp::write(satp::Satp::from_bits(0));
    }
}

/// Stage the CSR ceremony for a staged hart start entering S/HS mode: the
/// entry mirrors a fresh boot (`MPIE=1` so the entering software runs with
/// its own interrupt state, and the wake sources enabled).
///
/// # Safety
///
/// M-mode writes to this hart's S-mode and trap CSRs.
pub(crate) unsafe fn stage_next_mode(start_addr: usize, next_mode: mstatus::MPP) {
    unsafe {
        stage_smode_trap_state();
        mstatus::set_mpie();
        mstatus::set_mpp(next_mode);
        crate::csr::mie::set_machine_software();
        if crate::irq::get().is_some() {
            crate::csr::mie::set_machine_external();
        }
        if !init::has_sstc() {
            crate::csr::mie::set_machine_timer();
        }
        mepc::write(start_addr);
    }
}

/// Handles caller-saved time destinations before saving s0-s11.
/// For example, `rdtime a0` writes a caller-saved register and can return here;
/// `rdtime s0` needs the full frame and continues to normal emulation.
///
/// # Safety
/// Entry has initialized the caller-saved slots and trap CSRs, but not s0-s11.
/// Access only initialized fields through raw pointers; never borrow the full frame.
#[inline(never)]
pub(super) unsafe extern "C" fn try_fast_emulate_time(frame: *mut TrapFrame) -> bool {
    // SAFETY: these CSR slots are initialized by entry before this call.
    let (status, cause, inst, pc) = unsafe {
        (
            (*frame).mstatus,
            (*frame).mcause,
            (*frame).mtval as u32,
            (*frame).mepc,
        )
    };
    let rd = ((inst >> 7) & 31) as usize;
    if cause != ILLEGAL_INSTRUCTION
        || status & 0x1800 == 0x1800
        || inst & 0x000f_f07f != 0x2073
        || !matches!(rd, 0 | 1 | 5..=7 | 10..=17 | 28..=31)
    {
        return false;
    }
    let Some(value) = emulate::device_counter_word((inst >> 20) as u16) else {
        return false;
    };
    if let Some(counters) = crate::events::get() {
        counters.record_illegal_instruction();
    }
    if rd != 0 {
        // SAFETY: rd names an initialized caller-saved slot within the allocated frame.
        unsafe {
            core::ptr::addr_of_mut!((*frame).x)
                .cast::<usize>()
                .add(rd - 1)
                .write(value);
        }
    }
    // SAFETY: the pure 32-bit CSR read completed; other trap CSRs were not modified.
    unsafe {
        mepc::write(pc.wrapping_add(4));
    }
    true
}
