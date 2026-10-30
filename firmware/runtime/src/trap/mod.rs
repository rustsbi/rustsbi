//! Machine trap entry, dispatch, instruction emulation, and S/HS/VS forwarding.
//! Return state and original trap facts remain private to this subsystem.
//! Dispatch owns recovery and forwarding; emulation commits successful
//! instructions using the timer service or bounded lower-mode memory access.

mod decode;
mod dispatch;
mod emulate;
pub(crate) mod entry;
mod frame;
mod handlers;
mod init;
mod memory;
mod redirect;

pub use init::{TrapPolicy, init};

use crate::hart::TargetPrivilege;
use core::fmt;
pub use csr::values::trap::Exception;
use csr::{machine, values::Address};

/// Prints the current hart's `mcause`, `mepc`, and `mtval` through `print_fn`.
///
/// These values may describe an earlier trap rather than the current panic.
pub fn print_info(mut print_fn: impl FnMut(fmt::Arguments<'_>)) {
    let cause = machine::Cause::read().cause();
    let pc = usize::from(machine::ExceptionPc::read());
    let trap_value = usize::from(machine::TrapValue::read());
    print_fn(format_args!("mcause:  {cause:?}"));
    print_fn(format_args!("mepc:    {pc:#018x}"));
    print_fn(format_args!("mtval:   {trap_value:#018x}"));
}

/// Returns the privilege mode recorded in `mstatus.MPP`.
///
/// Returns `None` when the field holds a reserved encoding.
pub fn previous_privilege() -> Option<TargetPrivilege> {
    match machine::Status::read().read_field::<machine::MPP>() {
        machine::Privilege::User => Some(TargetPrivilege::User),
        machine::Privilege::Supervisor => Some(TargetPrivilege::Supervisor),
        machine::Privilege::Machine => Some(TargetPrivilege::Machine),
        machine::Privilege::Unknown(_) => None,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Fault {
    cause: Exception,
    address: Address,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Error {
    Unsupported,
    Fault(Fault),
}
