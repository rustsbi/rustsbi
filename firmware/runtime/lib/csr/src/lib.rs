//! Typed RISC-V CSR access for RustSBI firmware.
//!
//! Read snapshots and write values are ordinary data.
//! A complete write does not require a prior read.
//! Ordinary field edits through `modify` preserve other bits in the snapshot.
//! The read/modify/write sequence is not atomic.
//!
//! Run hardware operations at a privilege level that permits the selected CSR.
//! Unavailable accesses trap; this crate does not recover exceptions.
//! Unsafe operations require the register's documented safety conditions,
//! including valid memory, interrupt, and trap-entry state where applicable.
//! Constructing or editing a local write value remains safe and executes no
//! instruction.
//! Typed values constrain encodings; they do not prove that the hardware
//! transition is safe.
//!
//! # Pending interrupts and atomic claims
//!
//! The `mip` methods `set` and `clear` update software-controlled pending bits
//! with one CSRRS or CSRRC instruction, respectively.
//! A read of `SEIP` combines software state `B` and external signal `E`; these
//! instructions exclude `E` from the value used to update `B`.
//! A separate read/write can latch `E` into software pending state after the
//! external signal disappears.
//! Reading `mip` does not acknowledge or clear interrupts.
//! See the [machine interrupt
//! specification](https://docs.riscv.org/reference/isa/priv/machine.html#mip).
//!
//! Call the intended operation directly:
//!
//! ```no_run
//! # #![cfg_attr(any(target_arch = "riscv32", target_arch = "riscv64"), no_std)]
//! # #![cfg_attr(any(target_arch = "riscv32", target_arch = "riscv64"), no_main)]
//! # #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
//! # #[panic_handler]
//! # fn panic(_: &core::panic::PanicInfo<'_>) -> ! { loop {} }
//! # fn main() {
//! use csr::machine;
//! machine::InterruptPending::set::<machine::SSIP>();
//! machine::InterruptPending::clear::<machine::STIP>();
//! # }
//! ```
//!
//! The field selects the pending bit.
//! Callers do not construct a numeric bit mask.
//! An interrupt-enable field cannot be used as a pending field:
//!
//! ```compile_fail,E0277
//! # #![cfg_attr(any(target_arch = "riscv32", target_arch = "riscv64"), no_std)]
//! # #![cfg_attr(any(target_arch = "riscv32", target_arch = "riscv64"), no_main)]
//! # #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
//! # #[panic_handler]
//! # fn panic(_: &core::panic::PanicInfo<'_>) -> ! { loop {} }
//! # fn main() {
//! use csr::machine;
//! machine::InterruptPending::set::<machine::MSIE>();
//! # }
//! ```
//!
//! Field ranges belong to the codec implementation; fields expose no raw mask:
//!
//! ```compile_fail,E0599
//! # #![cfg_attr(any(target_arch = "riscv32", target_arch = "riscv64"), no_std)]
//! # #![cfg_attr(any(target_arch = "riscv32", target_arch = "riscv64"), no_main)]
//! # #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
//! # #[panic_handler]
//! # fn panic(_: &core::panic::PanicInfo<'_>) -> ! { loop {} }
//! # fn main() {
//! use csr::machine;
//! let mask = machine::SSIP::MASK;
//! # }
//! ```
//!
//! `mtopei` supports an ordinary read without acknowledgement.
//! Its `claim` method executes one `csrrw rd, mtopei, x0`, returning the
//! identity whose pending bit it clears.
//! A separate read and write could acknowledge a different interrupt.
//! See the [IMSIC
//! specification](https://docs.riscv.org/reference/aia/v1.0/IMSIC.html).
//!
//! ```no_run
//! # #![cfg_attr(any(target_arch = "riscv32", target_arch = "riscv64"), no_std)]
//! # #![cfg_attr(any(target_arch = "riscv32", target_arch = "riscv64"), no_main)]
//! # #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
//! # #[panic_handler]
//! # fn panic(_: &core::panic::PanicInfo<'_>) -> ! { loop {} }
//! # fn main() {
//! use csr::aia::machine::Mtopei;
//! if let Some(iid) = Mtopei::claim().iid() {
//!     let identity = iid.get();
//! }
//! # }
//! ```
//!
//! Ordinary instruction helpers cannot replace the atomic claim:
//!
//! ```compile_fail,E0277
//! # #![cfg_attr(any(target_arch = "riscv32", target_arch = "riscv64"), no_std)]
//! # #![cfg_attr(any(target_arch = "riscv32", target_arch = "riscv64"), no_main)]
//! # #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
//! # #[panic_handler]
//! # fn panic(_: &core::panic::PanicInfo<'_>) -> ! { loop {} }
//! # fn main() {
//! use csr::aia::machine::Mtopei;
//! fn ordinary_write<R: csr::Writeable>() {}
//! ordinary_write::<Mtopei>();
//! # }
//! ```
//!
//! A read snapshot cannot be written back through a whole-register write:
//!
//! ```compile_fail,E0599
//! # #![cfg_attr(any(target_arch = "riscv32", target_arch = "riscv64"), no_std)]
//! # #![cfg_attr(any(target_arch = "riscv32", target_arch = "riscv64"), no_main)]
//! # #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
//! # #[panic_handler]
//! # fn panic(_: &core::panic::PanicInfo<'_>) -> ! { loop {} }
//! # fn main() {
//! use csr::machine;
//! machine::InterruptPending::write(machine::InterruptPending::read());
//! # }
//! ```
//!
//! Generic read/modify/write is unavailable for this operation:
//!
//! ```compile_fail,E0599
//! # #![cfg_attr(any(target_arch = "riscv32", target_arch = "riscv64"), no_std)]
//! # #![cfg_attr(any(target_arch = "riscv32", target_arch = "riscv64"), no_main)]
//! # #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
//! # #[panic_handler]
//! # fn panic(_: &core::panic::PanicInfo<'_>) -> ! { loop {} }
//! # fn main() {
//! use csr::machine;
//! machine::InterruptPending::modify(|_, value| {
//!     value.write::<machine::SSIP>(machine::PendingState::Pending);
//! });
//! # }
//! ```
//!
//! Ordinary instruction helpers cannot bypass the dedicated write:
//!
//! ```compile_fail,E0277
//! # #![cfg_attr(any(target_arch = "riscv32", target_arch = "riscv64"), no_std)]
//! # #![cfg_attr(any(target_arch = "riscv32", target_arch = "riscv64"), no_main)]
//! # #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
//! # #[panic_handler]
//! # fn panic(_: &core::panic::PanicInfo<'_>) -> ! { loop {} }
//! # fn main() {
//! use csr::machine;
//! fn ordinary_write<R: csr::Writeable>() {}
//! ordinary_write::<machine::InterruptPending>();
//! # }
//! ```
//!
//! # Ordinary access examples
//!
//! ```no_run
//! # #![cfg_attr(any(target_arch = "riscv32", target_arch = "riscv64"), no_std)]
//! # #![cfg_attr(any(target_arch = "riscv32", target_arch = "riscv64"), no_main)]
//! # #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
//! # #[panic_handler]
//! # fn panic(_: &core::panic::PanicInfo<'_>) -> ! { loop {} }
//! # fn main() {
//! use csr::machine;
//! let hart = machine::HartId::read();
//! let mut value = machine::InterruptEnableWriteValue::disabled();
//! value.write::<machine::MSIE>(machine::InterruptState::Enabled);
//! // SAFETY:
//! // 1. This M-mode example assumes a valid machine software interrupt handler.
//! // 2. Its caller permits interrupt reentry throughout these operations.
//! unsafe { machine::InterruptEnable::write(value) };
//! // SAFETY: disabling this interrupt preserves the active execution context.
//! unsafe {
//!     machine::InterruptEnable::modify(|_, w| {
//!         w.write::<machine::MSIE>(machine::InterruptState::Disabled);
//!     });
//! }
//! # }
//! ```
//!
//! Read snapshots cannot be edited:
//!
//! ```compile_fail,E0599
//! # #![cfg_attr(any(target_arch = "riscv32", target_arch = "riscv64"), no_std)]
//! # #![cfg_attr(any(target_arch = "riscv32", target_arch = "riscv64"), no_main)]
//! # #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
//! # #[panic_handler]
//! # fn panic(_: &core::panic::PanicInfo<'_>) -> ! { loop {} }
//! # fn main() {
//! use csr::machine;
//! let mut status = machine::Status::read();
//! status.write::<machine::MIE>(machine::InterruptState::Disabled);
//! # }
//! ```
//!
//! Read encodings cannot be supplied where a write choice is required:
//!
//! ```compile_fail,E0308
//! # #![cfg_attr(any(target_arch = "riscv32", target_arch = "riscv64"), no_std)]
//! # #![cfg_attr(any(target_arch = "riscv32", target_arch = "riscv64"), no_main)]
//! # #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
//! # #[panic_handler]
//! # fn panic(_: &core::panic::PanicInfo<'_>) -> ! { loop {} }
//! # fn main() {
//! use csr::machine;
//! let mut status = machine::StatusWriteValue::for_return(machine::TargetPrivilege::Supervisor);
//! status.write::<machine::MPP>(machine::Privilege::Machine);
//! # }
//! ```
//!
//! Hardware status writes require their safety contract:
//!
//! ```compile_fail,E0133
//! # #![cfg_attr(any(target_arch = "riscv32", target_arch = "riscv64"), no_std)]
//! # #![cfg_attr(any(target_arch = "riscv32", target_arch = "riscv64"), no_main)]
//! # #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
//! # #[panic_handler]
//! # fn panic(_: &core::panic::PanicInfo<'_>) -> ! { loop {} }
//! # fn main() {
//! use csr::machine;
//! machine::Status::modify(|_, w| w.write::<machine::MIE>(machine::InterruptState::Enabled));
//! # }
//! ```
#![no_std]

mod macros;
mod value;

pub use value::{Readable, Writeable};

mod standard;
mod vendor;

pub use standard::isa::{
    privileged::{hypervisor, machine, supervisor, virtual_supervisor},
    unprivileged::counter_timer as user,
    values,
};
pub use vendor::thead;
pub use standard::aia;
