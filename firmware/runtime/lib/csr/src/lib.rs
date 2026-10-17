//! Typed RISC-V CSR instructions and field values.
//!
//! Register types identify hardware; they have no instances or acquisition step.
//! Read snapshots and write values are ordinary data. Their field operations
//! execute no hardware instructions.
//!
//! Ordinary `read()` observes the current hart's register without changing it.
//! Inaccessible or unavailable hardware can trap. Runtime owns probing,
//! recovery, privilege checks, and hart-local execution.
//!
//! Writes and operations with side effects are unsafe. Their callers must
//! establish current-hart access, coordinate conflicting operations, and
//! preserve the register's documented execution requirements.
//! Typed encodings do not prove that a hardware transition is safe.
//!
//! `modify` reads a snapshot and writes the edited value back.
//! This sequence is not atomic.
//!
//! `InterruptPending::set` and `clear` use CSRRS and CSRRC. They update
//! software-controlled pending bits without latching the external SEIP signal.
//! Whole-register writes and generic read/modify/write are unavailable.
//! See the [machine interrupt specification](https://docs.riscv.org/reference/isa/priv/machine.html#mip).
//!
//! `Mtopei::read` observes the top interrupt without acknowledgement.
//! `Mtopei::claim` executes one `csrrw rd, mtopei, x0` to read and acknowledge
//! the same identity. Generic writes and read/modify/write are unavailable.
//! See the [IMSIC specification](https://docs.riscv.org/reference/aia/v1.0/IMSIC.html).
//!
//! ```no_run
//! # #![cfg_attr(any(target_arch = "riscv32", target_arch = "riscv64"), no_std)]
//! # #![cfg_attr(any(target_arch = "riscv32", target_arch = "riscv64"), no_main)]
//! # #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
//! # #[panic_handler]
//! # fn panic(_: &core::panic::PanicInfo<'_>) -> ! { loop {} }
//! # fn main() {
//! use csr::machine;
//! let isa = machine::Isa::read();
//! let supports_supervisor = isa.has_supervisor();
//! // SAFETY: This M-mode example owns software pending-state delivery on its hart.
//! unsafe { machine::InterruptPending::set::<machine::SSIP>(); }
//! # }
//! ```
//!
//! Pending-bit operations reject interrupt-enable fields:
//!
//! ```compile_fail,E0277
//! # #![cfg_attr(any(target_arch = "riscv32", target_arch = "riscv64"), no_std)]
//! # #![cfg_attr(any(target_arch = "riscv32", target_arch = "riscv64"), no_main)]
//! # #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
//! # #[panic_handler]
//! # fn panic(_: &core::panic::PanicInfo<'_>) -> ! { loop {} }
//! # fn main() {
//! use csr::machine;
//! unsafe { machine::InterruptPending::set::<machine::MSIE>(); }
//! # }
//! ```
//!
//! Pending state has no whole-register write:
//!
//! ```compile_fail,E0599
//! # #![cfg_attr(any(target_arch = "riscv32", target_arch = "riscv64"), no_std)]
//! # #![cfg_attr(any(target_arch = "riscv32", target_arch = "riscv64"), no_main)]
//! # #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
//! # #[panic_handler]
//! # fn panic(_: &core::panic::PanicInfo<'_>) -> ! { loop {} }
//! # fn main() {
//! use csr::machine;
//! unsafe { machine::InterruptPending::write(machine::InterruptPending::read()); }
//! # }
//! ```
//!
//! Execution-environment writes require their safety contract:
//!
//! ```compile_fail,E0133
//! # #![cfg_attr(any(target_arch = "riscv32", target_arch = "riscv64"), no_std)]
//! # #![cfg_attr(any(target_arch = "riscv32", target_arch = "riscv64"), no_main)]
//! # #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
//! # #[panic_handler]
//! # fn panic(_: &core::panic::PanicInfo<'_>) -> ! { loop {} }
//! # fn main() {
//! use csr::machine;
//! machine::Status::modify(|_, value| {
//!     value.write::<machine::MIE>(machine::Interrupt::Enabled);
//! });
//! # }
//! ```
#![no_std]

mod macros;
mod value;

mod standard;
mod vendor;

pub use standard::isa::{
    privileged::{hypervisor, machine, supervisor, virtual_supervisor},
    unprivileged::counter_timer as user,
    values,
};
pub use vendor::thead;
pub use standard::aia;
