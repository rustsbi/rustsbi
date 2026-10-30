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
                status.write_field::<virtual_supervisor::SPIE>(read.read_field::<virtual_supervisor::SIE>());
                status.write_field::<virtual_supervisor::SIE>(machine::Interrupt::Disabled);
                status.write_field::<virtual_supervisor::SPP>(privilege);
            });
        }
        // Keep MPV set: mret enters the guest's virtual trap handler.
        frame
            .status
            .write_field::<machine::MPP>(machine::TargetPrivilege::Supervisor);
        frame.epc = virtual_supervisor::TrapVector::read()
            .read_field::<virtual_supervisor::BASE>()
            .address()
            .into();
        return;
    }
    if info.has_hypervisor {
        record_hypervisor_trap(info, privilege, secondary);
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
        .write_field::<machine::SPIE>(info.status.read_field::<machine::SIE>());
    frame
        .status
        .write_field::<machine::SIE>(machine::Interrupt::Disabled);
    frame.status.write_field::<machine::SPP>(privilege);
    frame
        .status
        .write_field::<machine::MPP>(machine::TargetPrivilege::Supervisor);
    frame.leave_virtualization();
    frame.epc = supervisor::TrapVector::read()
        .read_field::<supervisor::BASE>()
        .address()
        .into();
}

fn record_hypervisor_trap(
    info: &TrapInfo,
    privilege: machine::SupervisorPrivilege,
    secondary: Option<Fault>,
) {
    let virtualized = info.virtualized();
    let guest_address = if secondary.is_none() && info.guest_address() {
        machine::GuestAddress::GuestVirtual
    } else {
        machine::GuestAddress::Other
    };
    // Secondary faults here use one-stage S/U translation.
    let (trap_value, instruction) = match secondary {
        None => (info.tval2, info.instruction),
        Some(_) => (0, 0),
    };
    // SAFETY: The caller checked H is present. Dispatch owns this hart's
    // HS trap metadata and reproduces it from the immutable trap origin.
    unsafe {
        hypervisor::Status::modify(|_, status| {
            status.write_field::<hypervisor::SPV>(virtualized.into());
            if virtualized {
                status.write_field::<hypervisor::SPVP>(privilege);
            }
            status.write_field::<hypervisor::GVA>(guest_address);
        });
        hypervisor::TrapValue::write(trap_value.into());
        hypervisor::TrapInstruction::write(instruction.into());
    }
}
