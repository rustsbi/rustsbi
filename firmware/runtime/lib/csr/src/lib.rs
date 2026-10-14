//! Typed RISC-V CSR access for RustSBI firmware.
//!
//! Read snapshots and write values are ordinary data. A complete write does
//! not require a prior read. Field edits preserve untouched bits in the local value.
//! `modify` performs a non-atomic read followed by a write. Hardware may constrain
//! writable values or change register state between those operations.
//! Standard registers use `mode::Name`; vendor registers use `vendor::mode::Name`.
//! For example, use [`machine::CounterEnable`].
//!
//! ```no_run
//! use csr::machine;
//! let hart = machine::HartId::read();
//! let mut value = machine::InterruptEnableWriteValue::disabled();
//! value.write::<machine::MSIE>(machine::InterruptState::Enabled);
//! // SAFETY: a valid machine software interrupt handler is installed.
//! unsafe { machine::InterruptEnable::write(value) };
//! // SAFETY: disabling this interrupt preserves the active execution context.
//! unsafe {
//!     machine::InterruptEnable::modify(|_, w| {
//!         w.write::<machine::MSIE>(machine::InterruptState::Disabled);
//!     });
//! }
//! ```
//!
//! Read snapshots cannot be edited:
//!
//! ```compile_fail
//! use csr::machine;
//! let mut status = machine::Status::read();
//! status.write::<machine::MIE>(machine::InterruptState::Disabled);
//! ```
//!
//! Read encodings cannot be supplied where a write choice is required:
//!
//! ```compile_fail
//! use csr::machine;
//! let mut status = machine::StatusWriteValue::for_return(machine::TargetPrivilege::Supervisor);
//! status.write::<machine::MPP>(machine::Privilege::Machine);
//! ```
//!
//! Hardware status writes require their safety contract:
//!
//! ```compile_fail
//! use csr::machine;
//! machine::Status::modify(|_, w| w.write::<machine::MIE>(machine::InterruptState::Enabled));
//! ```
//!
//! Interrupt-pending updates require dedicated bit operations. A read-modify-write
//! could copy the hardware-ORed `SEIP` value into its software latch:
//!
//! ```compile_fail
//! use csr::machine;
//! machine::InterruptPending::modify(|_, w| {
//!     w.write::<machine::SSIP>(machine::PendingState::Pending);
//! });
//! ```

#![no_std]

mod macros;
mod value;
mod standard;

pub use value::{Readable, Writeable};
pub use standard::isa::{
    privileged::{machine},
    values,
};
