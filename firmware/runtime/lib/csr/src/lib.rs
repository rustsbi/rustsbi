//! Typed RISC-V CSR instructions and field values.
//!
//! A register such as `machine::Status` holds a complete register value.
//! `Status<Read>` represents an observed value; `Status<Write>` represents a
//! prepared write value. The default mode is `Read`.
//! The markers describe data, not permission to access hardware.
//! Field identifiers such as `machine::MIE` select a field and its semantic type.
//! Local `read_field` and `write_field` operations execute no hardware instructions.
//! Registers with shared scalar types, such as addresses and counters, use
//! those semantic types directly.
//! Registers whose complete value has register-specific meaning, such as
//! `machine::Isa`, use one plain value type for both reads and writes.
//!
//! ```
//! use csr::{Read, Write, machine::{Interrupt, MIE, MPP, Privilege, Status, TargetPrivilege}};
//!
//! let observed: Status<Read> = Status::from_bits(1 << 3);
//! assert_eq!(observed.read_field::<MIE>(), Interrupt::Enabled);
//!
//! let mut prepared = Status::<Write>::default();
//! prepared.write_field::<MPP>(TargetPrivilege::Supervisor);
//! prepared.write_field::<MIE>(Interrupt::Enabled);
//! assert_eq!(prepared.read_field::<MPP>(), Privilege::Supervisor);
//! assert_eq!(prepared.read_field::<MIE>(), Interrupt::Enabled);
//! ```
//!
//! `Default` starts a prepared value from zeroed local bits.
//! It does not describe a hardware reset state or a universally valid
//! configuration; callers set the fields their operation requires.
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
//! `InterruptPending::set_bit` and `clear_bit` use CSRRS and CSRRC. They update
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
//! let supports_supervisor = isa.has_extension('S');
//! // SAFETY: This M-mode example owns software pending-state delivery on its hart.
//! unsafe { machine::InterruptPending::set_bit::<machine::SSIP>(); }
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
//! unsafe { machine::InterruptPending::set_bit::<machine::MSIE>(); }
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
//!     value.write_field::<machine::MIE>(machine::Interrupt::Enabled);
//! });
//! # }
//! ```
//!
//! An observed value cannot be edited in place:
//!
//! ```compile_fail,E0599
//! use csr::machine::{Interrupt, MIE, Status};
//! let mut observed = Status::from_bits(0);
//! observed.write_field::<MIE>(Interrupt::Enabled);
//! ```
//!
//! A read value cannot be passed directly to a register write:
//!
//! ```compile_fail,E0308
//! use csr::machine::Status;
//! unsafe { Status::write(Status::from_bits(0)); }
//! ```
//!
//! Arbitrary bits do not construct a prepared status value:
//!
//! ```compile_fail,E0599
//! use csr::{Write, machine::Status};
//! let prepared = Status::<Write>::from_bits(usize::MAX);
//! ```
//!
//! A field belongs to its declared register:
//!
//! ```compile_fail,E0277
//! use csr::machine::{MSIE, Status};
//! let enabled = Status::from_bits(0).read_field::<MSIE>();
//! ```
//!
//! Reserved read encodings are not accepted as field write values:
//!
//! ```compile_fail,E0308
//! use csr::{Write, machine::{MPP, Privilege, Status}};
//! let mut prepared = Status::<Write>::default();
//! prepared.write_field::<MPP>(Privilege::Unknown(2));
//! ```
#![no_std]

mod macros;
mod value;

pub use value::{Read, Write};

mod standard;
mod vendor;

pub use standard::isa::{
    privileged::{hypervisor, machine, supervisor, virtual_supervisor},
    unprivileged::counter_timer as user,
    values,
};
pub use vendor::thead;
pub use standard::aia;
