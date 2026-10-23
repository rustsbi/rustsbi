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

use csr::values::Address;
pub use csr::values::trap::Exception;

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
