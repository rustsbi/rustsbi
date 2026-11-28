//! Software trap entry into S/HS or a delegated virtual supervisor.
use csr::values::trap::{CauseValue, TrapCause};
use csr::values::vector::VectorMode;
use csr::{hypervisor, machine, supervisor, virtual_supervisor};

use super::{
    Fault,
    frame::{TrapFrame, TrapInfo},
};

/// Entry address in the destination trap vector: vectored asynchronous
/// interrupts enter `BASE + 4 * code`, all other traps enter `BASE`. The
/// cause is the one written to the destination CSR, guest-visible for VS.
fn vector_entry(base: usize, mode: VectorMode, cause: CauseValue) -> usize {
    match (mode, cause.cause()) {
        (VectorMode::Vectored, TrapCause::Interrupt(_)) => {
            let interrupt = 1usize << (usize::BITS - 1);
            base.wrapping_add((usize::from(cause) & !interrupt).wrapping_mul(4))
        }
        _ => base,
    }
}

pub(super) fn redirect_trap(
    frame: &mut TrapFrame,
    info: &TrapInfo,
    privilege: machine::SupervisorPrivilege,
    secondary: Option<Fault>,
) {
    let (cause, tval) = match secondary {
        Some(fault) => (
            CauseValue::from(TrapCause::Exception(fault.cause)),
            usize::from(fault.address),
        ),
        None => (CauseValue::from(info.cause), info.tval),
    };
    let delegated = info.virtualized()
        && match cause.cause() {
            TrapCause::Exception(exception) => {
                // Saved virtualized state establishes that H is present.
                hypervisor::ExceptionDelegation::read().contains(exception)
            }
            TrapCause::Interrupt(_) => false,
        };
    if delegated {
        // SAFETY: H is present, and hedeleg selects a VS trap destination.
        unsafe {
            virtual_supervisor::ExceptionPc::write(info.epc.into());
            virtual_supervisor::Cause::write(cause);
            virtual_supervisor::TrapValue::write(tval.into());
            virtual_supervisor::Status::modify(|read, status| {
                status.set::<virtual_supervisor::SPIE>(read.get::<virtual_supervisor::SIE>());
                status.set::<virtual_supervisor::SIE>(machine::Interrupt::Disabled);
                status.set::<virtual_supervisor::SPP>(privilege);
            });
        }
        // Keep MPV set: mret enters the guest's virtual trap handler.
        frame
            .status
            .set::<machine::MPP>(machine::TargetPrivilege::Supervisor);
        let vector = virtual_supervisor::TrapVector::read();
        frame.epc = vector_entry(
            vector.get::<virtual_supervisor::BASE>().address().into(),
            vector.get::<virtual_supervisor::MODE>(),
            cause,
        );
    } else {
        if info.has_hypervisor {
            // SAFETY: Reproduce HS trap-entry metadata from the immutable origin.
            unsafe {
                hypervisor::Status::modify(|_, status| {
                    status.set::<hypervisor::SPV>(info.virtualized().into());
                    if info.virtualized() {
                        status.set::<hypervisor::SPVP>(privilege);
                    }
                    status.set::<hypervisor::GVA>(if secondary.is_none() && info.guest_address() {
                        machine::GuestAddress::GuestVirtual
                    } else {
                        machine::GuestAddress::Other
                    });
                });
            }
            // Secondary faults here use one-stage S/U translation.
            // SAFETY: Dispatch owns this hart's HS trap metadata.
            unsafe {
                hypervisor::TrapValue::write(
                    (if secondary.is_none() { info.tval2 } else { 0 }).into(),
                )
            };
            // SAFETY: Dispatch owns this hart's HS trap metadata.
            unsafe {
                hypervisor::TrapInstruction::write(
                    (if secondary.is_none() {
                        info.instruction
                    } else {
                        0
                    })
                    .into(),
                )
            };
        }
        // SAFETY: The saved context is lower-mode. These writes reproduce
        // hardware entry into the supervisor handler.
        unsafe {
            supervisor::ExceptionPc::write(info.epc.into());
            supervisor::Cause::write(cause);
            supervisor::TrapValue::write(tval.into());
        }
        frame
            .status
            .set::<machine::SPIE>(info.status.get::<machine::SIE>());
        frame
            .status
            .set::<machine::SIE>(machine::Interrupt::Disabled);
        frame.status.set::<machine::SPP>(privilege);
        frame
            .status
            .set::<machine::MPP>(machine::TargetPrivilege::Supervisor);
        frame.leave_virtualization();
        let vector = supervisor::TrapVector::read();
        frame.epc = vector_entry(
            vector.get::<supervisor::BASE>().address().into(),
            vector.get::<supervisor::MODE>(),
            cause,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use csr::values::trap::{Exception, Interrupt};

    fn interrupt(code: usize) -> CauseValue {
        CauseValue::from(TrapCause::Interrupt(Interrupt::Unknown(code)))
    }

    #[test]
    fn vectored_interrupts_enter_at_base_plus_four_times_cause() {
        // Supervisor timer is code 5, the guest-visible VS numbering.
        let timer = CauseValue::from(TrapCause::Interrupt(Interrupt::SupervisorTimer));
        assert_eq!(
            vector_entry(0x1000, VectorMode::Vectored, timer),
            0x1000 + 4 * 5
        );
        assert_eq!(
            vector_entry(0x1000, VectorMode::Vectored, interrupt(9)),
            0x1000 + 4 * 9
        );
        // Platform causes above the standard codes keep their raw number.
        assert_eq!(
            vector_entry(0x1000, VectorMode::Vectored, interrupt(300)),
            0x1000 + 4 * 300
        );
    }

    #[test]
    fn direct_and_reserved_modes_and_exceptions_enter_at_base() {
        let timer = CauseValue::from(TrapCause::Interrupt(Interrupt::SupervisorTimer));
        assert_eq!(vector_entry(0x1000, VectorMode::Direct, timer), 0x1000);
        assert_eq!(vector_entry(0x1000, VectorMode::Unknown(2), timer), 0x1000);
        let illegal = CauseValue::from(TrapCause::Exception(Exception::IllegalInstruction));
        assert_eq!(vector_entry(0x1000, VectorMode::Vectored, illegal), 0x1000);
    }

    #[test]
    fn vectored_entries_wrap_near_the_address_space_end() {
        let base = usize::MAX - 8;
        assert_eq!(vector_entry(base, VectorMode::Vectored, interrupt(5)), 11);
    }
}
