//! Machine trap classification and selection of the handling path.

use crate::{boot, events, irq::LocalIrqDisabled, recovery, timer};
use csr::{
    Read, Write,
    standard::isa::{
        privileged::machine::{Mcause, Mepc, Mie, Mtinst, Mtval, Mtval2, Mtvec},
        values::{
            address::Address,
            status::{Privilege, SupervisorPrivilege},
            trap::{
                CauseValue, Exception, Interrupt, TrapCause, TrapInstructionBits, TrapValueBits,
            },
            vector::{VectorBase, VectorMode},
        },
    },
};

use super::{
    Error, FaultSnapshot, emulate,
    frame::{GeneralRegs, SavedTrap, TrapFrame, TrapInfo},
    handlers, redirect,
};

/// Reads `mcause`, `mepc`, and `mtval` from the current hart.
///
/// A nested trap can replace these CSRs between reads. Capture them with
/// nested traps excluded when a coherent report is required.
#[inline]
pub fn current_fault_snapshot() -> FaultSnapshot {
    FaultSnapshot {
        mcause: Mcause::read(),
        mepc: Mepc::read(),
        mtval: Mtval::read(),
    }
}

/// Dispatches the current hart's saved machine trap.
///
/// # Safety
///
/// 1. Enter through `entry` with a complete, matching trap record on the Runtime stack.
/// 2. Keep `MIE` and `MPRV` clear and `mscratch` zero on entry.
/// 3. Every optional CSR indicated by the record must exist on this hart.
pub(super) unsafe extern "C" fn trap_dispatch(saved: &mut SavedTrap) {
    let frame = &mut saved.frame;
    let info = &saved.info;
    match info.cause() {
        TrapCause::Interrupt(interrupt) => {
            // SAFETY: Runtime entry keeps MIE clear throughout interrupt dispatch.
            let irq = unsafe { LocalIrqDisabled::assume_disabled() };
            let result = match interrupt {
                Interrupt::MachineSoftware => handlers::software_interrupt(frame, &irq),
                Interrupt::MachineTimer => {
                    timer::on_interrupt(&irq);
                    Ok(())
                }
                Interrupt::MachineExternal => handlers::external_interrupt(frame, &irq),
                _ => fatal(&frame.general, info, None),
            };
            if let Err(error) = result {
                fatal(&frame.general, info, Some(error));
            }
        }
        TrapCause::Exception(exception) => {
            if exception == Exception::DoubleTrap && !info.virtualized() {
                // Ssdbltrp implements mtval2 even on harts without H.
                let info = TrapInfo {
                    tval2: Mtval2::read().bits(),
                    ..*info
                };
                fatal(&frame.general, &info, None);
            }
            let privilege = match info.origin() {
                Ok(Privilege::Machine) => {
                    let Some(pc) = recovery::find_fixup(info.epc, info.cause) else {
                        fatal(&frame.general, info, None);
                    };
                    frame.general.t2 = info.cause;
                    frame.general.t3 = info.tval;
                    frame.epc = pc;
                    return;
                }
                Ok(Privilege::User) => SupervisorPrivilege::User,
                Ok(Privilege::Supervisor) => SupervisorPrivilege::Supervisor,
                Err(_) => fatal(&frame.general, info, None),
            };
            // SAFETY: Entry preserved the return context and established Runtime recovery.
            unsafe { handle_lower_exception(frame, info, exception, privilege) };
        }
    }
}

/// Handles a lower-mode exception or forwards it to the appropriate supervisor.
///
/// # Safety
///
/// Retain the trap entry state and recovery ABI required by [`redirect::redirect_trap`].
/// `privilege` and `exception` must describe the interrupted context in `info`.
unsafe fn handle_lower_exception(
    frame: &mut TrapFrame,
    info: &TrapInfo,
    exception: Exception,
    privilege: SupervisorPrivilege,
) {
    if let Some(counters) = events::get() {
        counters.record(exception);
    }
    // Guests always forward to HS or VS; their ecalls never invoke machine SBI.
    let result = if info.virtualized() {
        Err(Error::Unsupported)
    } else {
        match exception {
            Exception::SupervisorEcall => handlers::sbi_ecall(frame, info),
            Exception::IllegalInstruction => emulate::emulate_time_read(frame, info),
            Exception::LoadMisaligned => emulate::emulate_load(frame, info),
            Exception::StoreMisaligned => emulate::emulate_store(frame, info),
            _ => Err(Error::Unsupported),
        }
    };
    if let Err(error) = result {
        let secondary = match error {
            Error::Fault(fault) => Some(fault),
            Error::Unsupported => None,
            Error::Runtime(error) => fatal(&frame.general, info, Some(error)),
        };
        if let Err(error) =
            // SAFETY: Dispatch retains the original context and recovery vector,
            // with machine interrupts masked and MPRV clear.
            unsafe { redirect::redirect_trap(frame, info, privilege, exception, secondary) }
        {
            fatal(&frame.general, info, Some(error));
        }
    }
}

/// Delivers a software-detected S/HS double trap to Runtime's fatal M-mode handler.
#[cold]
pub(super) fn fatal_double_trap(
    frame: &TrapFrame,
    info: &TrapInfo,
    unexpected: Exception,
    tval: usize,
    instruction: usize,
) -> ! {
    let info = TrapInfo {
        cause: Exception::DoubleTrap.code(),
        tval,
        tval2: unexpected.code(),
        instruction,
        ..*info
    };
    // SAFETY: SDT was set, so Ssdbltrp supplies mtval2. Dispatch owns the
    // trap CSRs through fail-stop, including the fault snapshot read by panic.
    unsafe {
        Mepc::write(Address::from_bits(info.epc));
        Mcause::write(CauseValue::from_bits(info.cause));
        Mtval::write(TrapValueBits::from_bits(info.tval));
        Mtval2::write(TrapValueBits::from_bits(info.tval2));
        if info.has_hypervisor {
            Mtinst::write(TrapInstructionBits::from_bits(info.instruction));
        }
    }
    fatal(&frame.general, &info, None)
}

/// Terminates this hart after an unhandled trap.
#[cold]
fn fatal(registers: &GeneralRegs, info: &TrapInfo, error: Option<crate::Error>) -> ! {
    let mut vector = Mtvec::zero();
    vector.set(
        Mtvec::BASE,
        VectorBase::new(Address::from_bits(boot::fail_stop as *const () as usize))
            .expect("aligned fail-stop vector"),
    );
    vector.set(Mtvec::MODE, VectorMode::Direct);
    // SAFETY:
    // 1. fail_stop needs no stack and never returns, including during panic reporting.
    // 2. Disabling machine interrupt sources preserves the current exclusion.
    unsafe {
        Mtvec::write(vector);
        Mie::write(Mie::zero());
    }
    panic!(
        "unhandled machine trap: pc={:#x}, status={:#x}, cause={:#x}, tval={:#x}, tval2={:#x}, instruction={:#x}, error={error:?}\n{:#x?}",
        info.epc,
        info.status_bits(),
        info.cause,
        info.tval,
        info.tval2,
        info.instruction,
        registers,
    )
}
