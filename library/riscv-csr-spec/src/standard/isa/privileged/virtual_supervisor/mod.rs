//! Virtual-supervisor CSR definitions.
//!
//! [`vsip` and `vsie`](https://docs.riscv.org/reference/isa/v20260120/priv/hypervisor.html#14-1-2-12-virtual-supervisor-interrupt-vsip-and-vsie-registers)
//! expose pending and enabled guest interrupts according to `hideleg` and the
//! [AIA interrupt-virtualization controls](https://docs.riscv.org/reference/aia/v1.0/VSLevel.html#5-1-3-2-virtual-interrupts-for-vs-level).
//!
//! Coordinate updates to `Vsscratch`, `Vscause`, and `Vstval` with active VS-mode
//! trap handling.

mod address_translation;
mod indirect;
mod timer_compare;
mod trap_handling;
mod trap_setup;

pub use address_translation::*;
pub use indirect::*;
pub use timer_compare::*;
pub use trap_handling::*;
pub use trap_setup::*;
