//! Software trap entry into S/HS or a delegated virtual supervisor.
use csr::values::trap::{CauseValue, TrapCause};
use csr::{hypervisor, machine, supervisor, virtual_supervisor};

use super::{
    Fault,
    frame::{TrapFrame, TrapInfo},
};

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
                status.write::<virtual_supervisor::SPIE>(read.read::<virtual_supervisor::SIE>());
                status.write::<virtual_supervisor::SIE>(machine::Interrupt::Disabled);
                status.write::<virtual_supervisor::SPP>(privilege);
            });
        }
        // Keep MPV set: mret enters the guest's virtual trap handler.
        frame
            .status
            .write::<machine::MPP>(machine::TargetPrivilege::Supervisor);
        frame.epc = virtual_supervisor::TrapVector::read()
            .read::<virtual_supervisor::BASE>()
            .address()
            .into();
    } else {
        if info.has_hypervisor {
            // SAFETY: Reproduce HS trap-entry metadata from the immutable origin.
            unsafe {
                hypervisor::Status::modify(|_, status| {
                    status.write::<hypervisor::SPV>(info.virtualized().into());
                    if info.virtualized() {
                        status.write::<hypervisor::SPVP>(privilege);
                    }
                    status.write::<hypervisor::GVA>(
                        if secondary.is_none() && info.guest_address() {
                            machine::GuestAddress::GuestVirtual
                        } else {
                            machine::GuestAddress::Other
                        },
                    );
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
            .write::<machine::SPIE>(info.status.read::<machine::SIE>());
        frame
            .status
            .write::<machine::SIE>(machine::Interrupt::Disabled);
        frame.status.write::<machine::SPP>(privilege);
        frame
            .status
            .write::<machine::MPP>(machine::TargetPrivilege::Supervisor);
        frame.leave_virtualization();
        frame.epc = supervisor::TrapVector::read()
            .read::<supervisor::BASE>()
            .address()
            .into();
    }
}
