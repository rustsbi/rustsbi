//! Typed RISC-V CSR instructions and field values.
//!
//! A register such as `machine::Status` holds a complete register value.
//! `Status<Read>` represents an observed value; `Status<Write>` represents a
//! prepared write value.
//! The default mode is `Read`.
//! The markers describe data, not permission to access hardware.
//! Field identifiers such as `machine::MIE` select a field and its semantic type.
//! Local `get` and `set` operations execute no hardware instructions.
//! Registers with shared scalar types, such as addresses and counters, use
//! those semantic types directly.
//! Registers whose complete value has register-specific meaning, such as
//! `machine::Isa`, use one plain value type for both reads and writes.
//!
//! ```
//! use csr::{Read, Write, machine::{Interrupt, MIE, MPP, Privilege, Status, TargetPrivilege}};
//!
//! let observed: Status<Read> = Status::from_bits(1 << 3);
//! assert_eq!(observed.get::<MIE>(), Interrupt::Enabled);
//!
//! let mut prepared = Status::<Write>::default();
//! prepared.set::<MPP>(TargetPrivilege::Supervisor);
//! prepared.set::<MIE>(Interrupt::Enabled);
//! assert_eq!(prepared.get::<MPP>(), Privilege::Supervisor);
//! assert_eq!(prepared.get::<MIE>(), Interrupt::Enabled);
//! ```
//!
//! `Default` starts a prepared value from zeroed local bits.
//! It does not describe a hardware reset state or a universally valid
//! configuration; callers set the fields their operation requires.
//! Some registers also allow a read value to be converted to a write value,
//! preserving every bit, including reserved encodings. The register's write
//! contract still applies to the complete value.
//!
//! Ordinary `read()` observes the current hart's register without changing it.
//! Inaccessible or unavailable hardware can trap.
//! Runtime owns probing, recovery, privilege checks, and hart-local execution.
//!
//! Writes and operations with side effects are unsafe.
//! Their callers must establish current-hart access, coordinate conflicting
//! operations, and preserve the register's documented execution requirements.
//! Typed encodings do not prove that a hardware transition is safe.
//!
//! `modify` reads a snapshot and writes the edited value back.
//! This sequence is not atomic.
//!
//! See [`machine::InterruptPending`] and [`aia::machine::Mtopei`] for
//! registers that require dedicated operations.
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
//!     value.set::<machine::MIE>(machine::Interrupt::Enabled);
//! });
//! # }
//! ```
//!
//! An observed value cannot be edited in place:
//!
//! ```compile_fail,E0599
//! use csr::machine::{Interrupt, MIE, Status};
//! let mut observed = Status::from_bits(0);
//! observed.set::<MIE>(Interrupt::Enabled);
//! ```
//!
//! A read value cannot be passed directly to a register write:
//!
//! ```compile_fail,E0308
//! use csr::machine::Status;
//! unsafe { Status::write(Status::from_bits(0)); }
//! ```
//!
//! Reserved read encodings are not accepted as field write values:
//!
//! ```compile_fail,E0308
//! use csr::{Write, machine::{MPP, Privilege, Status}};
//! let mut prepared = Status::<Write>::default();
//! prepared.set::<MPP>(Privilege::Unknown(2));
//! ```
#![no_std]

mod macros;
mod value;

pub use value::{Read, Write};

mod standard;
pub mod vendor;

pub use standard::isa::{
    privileged::{hypervisor, machine, supervisor, virtual_supervisor},
    unprivileged::counter_timer as user,
    values,
};
pub use standard::aia;

#[cfg(test)]
mod tests {
    use super::{hypervisor, machine, supervisor, user, values::StateEnableSet};

    #[test]
    fn rv32_high_register_definitions_are_available() {
        assert_eq!(
            [
                hypervisor::StateEnable0High::NUMBER,
                hypervisor::StateEnable1High::NUMBER,
                hypervisor::StateEnable2High::NUMBER,
                hypervisor::StateEnable3High::NUMBER,
                machine::StateEnable0High::NUMBER,
                machine::StateEnable1High::NUMBER,
                machine::StateEnable2High::NUMBER,
                machine::StateEnable3High::NUMBER,
                machine::StatusHigh::NUMBER,
                machine::CycleCounterHigh::NUMBER,
                machine::InstructionCounterHigh::NUMBER,
                machine::PerformanceCounterHigh::<3>::NUMBER,
                machine::PerformanceEventHigh::<3>::NUMBER,
                supervisor::TimerCompareHigh::NUMBER,
                user::TimeHigh::NUMBER,
            ],
            [
                0x61c, 0x61d, 0x61e, 0x61f, 0x31c, 0x31d, 0x31e, 0x31f, 0x310, 0xb80, 0xb82, 0xb83,
                0x723, 0x15d, 0xc81,
            ]
        );
        assert_eq!(usize::from(StateEnableSet::lower_registers_high()), 1 << 31);

        let mut high = machine::StatusHigh::<super::Write>::default();
        high.set::<machine::GVAH>(machine::GuestAddress::GuestVirtual);
        high.set::<machine::MPVH>(machine::Virtualization::On);
        assert_eq!(usize::from(high), (1 << 6) | (1 << 7));
        #[cfg(target_pointer_width = "64")]
        {
            let mut status = machine::Status::<super::Write>::default();
            status.set::<machine::GVA>(machine::GuestAddress::GuestVirtual);
            status.set::<machine::MPV>(machine::Virtualization::On);
            assert_eq!(usize::from(status), (1 << 38) | (1 << 39));
        }
    }
}
