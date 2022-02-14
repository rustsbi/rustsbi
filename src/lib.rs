//! A minimal RISC-V's SBI implementation library in Rust.
//!
//! # What is RISC-V SBI?
//!
//! RISC-V SBI is short for RISC-V Supervisor Binary Interface. SBI acts as a bootloader environment to your operating system kernel.
//! A SBI implementation will bootstrap your kernel, and provide an environment when your kernel is running.
//!
//! More generally, The SBI allows supervisor-mode (S-mode or VS-mode) software to be portable across
//! all RISC-V implementations by defining an abstraction for platform (or hypervisor) specific functionality.
//!
//! # How to use RustSBI in your supervisor software
//!
//! SBI features include boot sequence and a kernel environment. To bootstrap your kernel,
//! place kernel into RustSBI implementation defined address, then RustSBI will prepare an
//! environment and jump to this address.
//!
//! ## Make SBI environment calls
//!
//! To use the kernel environment, you either use SBI calls or emulated instructions.
//! SBI calls are similar to operating systems' `syscall`s. RISC-V SBI defined many SBI modules,
//! and in each module there are different functions, you should pick a function before calling.
//! Then, you should prepare some parameters, whose definition are not the same among functions.
//!
//! Now you have a module number, a function number, and a few SBI call parameters.
//! You invoke a special `ecall` instruction on supervisor level, and it will trap into machine level
//! SBI implementation. It will handle your `ecall`, similar to your kernel handling system calls
//! from user level.
//!
//! SBI functions return two values other than one. First value will be an error number,
//! it will tell if SBI call have succeeded, or which error have occurred.
//! Second value is the real return value, its meaning is different according to which function you calls.
//!
//! ## Call SBI in different programming languages
//!
//! Making SBI calls are similar to making system calls.
//!
//! Module number is required to put on register `a7`, function number on `a6`.
//! Parameters should be placed from `a0` to `a5`, first into `a0`, second into `a1`, etc.
//! Unused parameters can be set to any value or leave untouched.
//!
//! After registers are ready, invoke an instruction called `ecall`.
//! Then, the return value is placed into `a0` and `a1` registers.
//! The error value could be read from `a0`, and return value is placed into `a1`.
//!
//! In Rust, here is an example to call SBI functions using inline assembly:
//!
//! ```no_run
//! # #[repr(C)] struct SbiRet { error: usize, value: usize }
//! # const EXTENSION_BASE: usize = 0x10;
//! # const FUNCTION_BASE_GET_SPEC_VERSION: usize = 0x0;
//! #[inline(always)]
//! fn sbi_call(extension: usize, function: usize, arg0: usize, arg1: usize) -> SbiRet {
//!     let (error, value);
//!     match () {
//!         #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
//!         () => unsafe { asm!(
//!             "ecall",
//!             in("a0") arg0, in("a1") arg1,
//!             in("a6") function, in("a7") extension,
//!             lateout("a0") error, lateout("a1") value,
//!         ) },
//!         #[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
//!         () => {
//!             drop((extension, function, arg0, arg1));
//!             unimplemented!("not RISC-V instruction set architecture")
//!         }
//!     };
//!     SbiRet { error, value }
//! }
//!
//! #[inline]
//! pub fn get_spec_version() -> SbiRet {
//!     sbi_call(EXTENSION_BASE, FUNCTION_BASE_GET_SPEC_VERSION, 0, 0)
//! }
//! ```
//!
//! Complex SBI functions may fail. In this example we only take the value, but in complete designs
//! we should handle the `error` value returned from SbiRet.
//!
//! You may use other languages to call SBI environment. In C programming language, we can call like this:
//!
//! ```text
//! #define SBI_CALL(module, funct, arg0, arg1, arg2, arg3) ({ \
//!     register uintptr_t a0 asm ("a0") = (uintptr_t)(arg0); \
//!     register uintptr_t a1 asm ("a1") = (uintptr_t)(arg1); \
//!     register uintptr_t a2 asm ("a2") = (uintptr_t)(arg2); \
//!     register uintptr_t a3 asm ("a3") = (uintptr_t)(arg3); \
//!     register uintptr_t a7 asm ("a6") = (uintptr_t)(funct); \
//!     register uintptr_t a7 asm ("a7") = (uintptr_t)(module); \
//!     asm volatile ("ecall" \
//!         : "+r" (a0), "+r" (a1) \
//!         : "r" (a1), "r" (a2), "r" (a3), "r" (a6), "r" (a7) \
//!         : "memory") \
//!     {a0, a1}; \
//! })
//!
//! #define SBI_CALL_0(module, funct) SBI_CALL(module, funct, 0, 0, 0, 0)
//!
//! static inline sbiret get_spec_version() {
//!     SBI_CALL_0(EXTENSION_BASE, FUNCTION_BASE_GET_SPEC_VERSION)
//! }
//! ```
//!
//! # Non-features
//!
//! RustSBI is designed to strictly adapt to the RISC-V Supervisor Binary Interface specification.
//! Other features useful in developing kernels and hypervisors maybe included in other Rust
//! ecosystem crates other than this package.
//!
//! ## Hardware discovery and feature detection
//!
//! According to the RISC-V SBI specification, SBI does not specify any method for hardware discovery.
//! The supervisor software must rely on the other industry standard hardware
//! discovery methods (i.e. Device Tree or ACPI) for that.
//!
//! To detect any feature under bare metal or under supervisor level, developers may depend on
//! any hardware discovery methods, or use try-execute-trap method to detect any instructions or
//! CSRs. If SBI is implemented in user level emulators, it may requires to depend on operating
//! system calls or use the signal trap method to detect any RISC-V core features.
//!
//! ## Where can I get RustSBI binary file for XX platform?
//!
//! RustSBI is designed to be a library instead of providing any binary files to specific platforms.
//! Chip or board manufacturers should provide their own SBI implementation project using RustSBI as a dependency.
//!
//! The RustSBI team provides reference implementation for several platforms, but they are for evaluation
//! only and should not be used in production.
//! RustSBI itself cannot decide for all arbitrary users, so developers are encouraged to use RustSBI
//! as a Rust crate dependency to support their own SBI implementation,
//! other than use reference implementation directly when in production.
//! SBI feature demands are different among users, one feature would be useful for this user,
//! but it will be considered not useful and takes up lots of flash room for other users.
//!
//! RustSBI is not designed to include all platforms available in official repository.
//! For an actual platform users may consult board or SoC manufacturer other than RustSBI repository itself.
//!
//! The reason to that is that if some repository includes all platforms it support,
//! there could be lots of non technical reasons that will bar one or a certain group of developers
//! from merging their code into main or upstream branches.
//! For example, if a draft version of actual platform is produced, it will mean to write for one draft version as well
//! as long as this software is under maintenance. As software developer may not want to write for it,
//! it's better to include minimal feature in core repository, and leave other features for downstream developers.
//!
//! # Notes for RustSBI developers
//!
//! This library adapts to embedded Rust's `embedded-hal` crate to provide basic SBI features.
//! When building for own platform, implement traits in this library and pass them to the functions
//! begin with `init`. After that, you may call `rustsbi::ecall` in your own exception handler
//! which would dispatch parameters from supervisor to the traits to execute SBI functions.
//!
//! The library also implements useful functions which may help with platform specific binaries.
//! The `enter_privileged` maybe used to enter the operating system after the initialization
//! process is finished. The `LOGO` should be printed if necessary when the binary is initializing.
//!
//! Note that this crate is a library which contains common building blocks in SBI implementation.
//! It is not intended to be used directly; users should build own platforms with this library.
//! RustSBI provides implementations on common platforms in separate platform crates.

#![no_std]
#![feature(ptr_metadata)]

extern crate alloc;

#[doc(hidden)]
#[macro_use]
pub mod legacy_stdio;
mod ecall;
mod extension;
mod hart_mask;
mod hsm;
mod ipi;
mod logo;
mod pmu;
mod privileged;
#[doc(hidden)]
pub mod reset;
mod rfence;
mod timer;

mod util;

const SBI_SPEC_MAJOR: usize = 0;
const SBI_SPEC_MINOR: usize = 3;

// RustSBI implementation ID: 4
// Ref: https://github.com/riscv-non-isa/riscv-sbi-doc/pull/61
const IMPL_ID_RUSTSBI: usize = 4;
// Read from env!("CARGO_PKG_VERSION")
const RUSTSBI_VERSION_MAJOR: usize =
    (env!("CARGO_PKG_VERSION_MAJOR").as_bytes()[0] - b'0') as usize;
const RUSTSBI_VERSION_MINOR: usize =
    (env!("CARGO_PKG_VERSION_MINOR").as_bytes()[0] - b'0') as usize;
const RUSTSBI_VERSION_PATCH: usize =
    (env!("CARGO_PKG_VERSION_PATCH").as_bytes()[0] - b'0') as usize;
#[rustfmt::skip]
const RUSTSBI_VERSION: usize =
    (RUSTSBI_VERSION_MAJOR << 16) + (RUSTSBI_VERSION_MINOR << 8) + RUSTSBI_VERSION_PATCH;
/// RustSBI version as a string.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub use ecall::handle_ecall as ecall;
pub use ecall::SbiRet;
pub use hart_mask::HartMask;
pub use hsm::{init_hsm, Hsm};
pub use ipi::{init_ipi, Ipi};
#[doc(hidden)]
pub use legacy_stdio::{legacy_stdio_getchar, legacy_stdio_putchar};
pub use logo::LOGO;
pub use pmu::{init_pmu, Pmu};
pub use privileged::enter_privileged;
pub use reset::{init_reset, Reset};
pub use rfence::{init_rfence as init_remote_fence, Rfence as Fence};
pub use timer::{init_timer, Timer};
