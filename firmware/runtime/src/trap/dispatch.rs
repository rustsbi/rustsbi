//! Machine trap classification and selection of the handling path.
use csr::{
    machine,
    values::{
        trap::{CauseValue, Exception, Interrupt, TrapCause},
        vector::{TargetVectorMode, VectorBase},
    },
};

use super::{
    Error, emulate,
    frame::{GeneralRegs, SavedTrap, TrapFrame, TrapInfo},
    handlers, redirect,
};

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
    match CauseValue::from(info.cause).cause() {
        TrapCause::Interrupt(interrupt) => {
            // SAFETY: Runtime entry keeps MIE clear throughout interrupt dispatch.
            let irq = unsafe { crate::irq::LocalIrqDisabled::assume_disabled() };
            match interrupt {
                Interrupt::MachineSoftware => handlers::software_interrupt(frame, &irq),
                Interrupt::MachineTimer => {
                    crate::boot::timer()
                        .expect("Timer service not published")
                        .on_interrupt(&irq);
                }
                Interrupt::MachineExternal => handlers::external_interrupt(frame, &irq),
                _ => {
                    let code = info.cause & !(1usize << (usize::BITS - 1));
                    if !crate::machine_irq::get()
                        .is_some_and(|policy| policy.handle_interrupt(code))
                    {
                        fatal(&frame.general, info);
                    }
                }
            }
        }
        TrapCause::Exception(exception) => {
            let privilege = match info.origin() {
                machine::Privilege::Machine => {
                    let Some(pc) = crate::recovery::find_fixup(info.epc, info.cause) else {
                        fatal(&frame.general, info);
                    };
                    frame.general.t2 = info.cause;
                    frame.general.t3 = info.tval;
                    frame.epc = pc;
                    return;
                }
                machine::Privilege::User => machine::SupervisorPrivilege::User,
                machine::Privilege::Supervisor => machine::SupervisorPrivilege::Supervisor,
                machine::Privilege::Unknown(_) => fatal(&frame.general, info),
            };
            handle_lower_exception(frame, info, exception, privilege);
        }
    }
}

fn handle_lower_exception(
    frame: &mut TrapFrame,
    info: &TrapInfo,
    exception: Exception,
    privilege: machine::SupervisorPrivilege,
) {
    if let Some(counters) = crate::events::get() {
        counters.record(exception);
    }
    // Guests always forward to HS or VS; their ecalls never invoke machine SBI.
    let result = if info.virtualized() {
        Err(Error::Unsupported)
    } else {
        match exception {
            Exception::SupervisorEcall => {
                handlers::sbi_ecall(frame);
                return;
            }
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
        };
        redirect::redirect_trap(frame, info, privilege, secondary);
    }
}

/// Terminates this hart after an unhandled trap.
#[cold]
fn fatal(registers: &GeneralRegs, info: &TrapInfo) -> ! {
    // SAFETY:
    // 1. fail_stop needs no stack and never returns, including during panic reporting.
    // 2. Disabling machine interrupt sources preserves the current exclusion.
    unsafe {
        machine::TrapVector::write(machine::TrapVectorWriteValue::new(
            VectorBase::new((crate::boot::fail_stop as *const () as usize).into())
                .expect("aligned fail-stop vector"),
            TargetVectorMode::Direct,
        ));
        machine::InterruptEnable::write(csr::values::trap::InterruptSet::empty());
    }
    #[cfg(target_pointer_width = "32")]
    let status_high = usize::from(info.status_high);
    #[cfg(target_pointer_width = "64")]
    let status_high = info.status_high;
    panic!(
        "unhandled machine trap: pc={:#x}, status={:#x}, status_high={:#x}, cause={:#x}, tval={:#x}, tval2={:#x}, instruction={:#x}\n{:#x?}",
        info.epc,
        usize::from(info.status),
        status_high,
        info.cause,
        info.tval,
        info.tval2,
        info.instruction,
        registers,
    )
}
