//! A minimal RISC-V's SBI implementation library in Rust.
//!
//! *Note: If you are a user looking for binary distribution download for RustSBI, you may consider
//! to use the RustSBI Prototyping System which will provide binaries for each platforms.
//! If you are a vendor or contributor who wants to adapt RustSBI to your new product or board,
//! you may consider adapting the Prototyping System first to get your board adapted in an afternoon;
//! you are only advised to build a discrete crate if your team have a lot of time working on this board.*
//!
//! *For more details on binary downloads the the RustSBI Prototyping System,
//! see section [Prototyping System vs discrete packages](#download-binary-file-the-prototyping-system-vs-discrete-packages).*
//!
//! The crate `rustsbi` acts as core trait and instance abstraction of RustSBI ecosystem.
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
//! # Hypervisor and emulator development with RustSBI
//!
//! RustSBI crate supports to develop RISC-V emulators, and both Type-1 and Type-2 hypervisors.
//! Hypervisor developers may find it easy to handle standard SBI functions with an instance
//! based RustSBI interface.
//!
//! ## Hypervisors using RustSBI
//!
//! Both Type-1 and Type-2 hypervisors on RISC-V runs on HS-mode hardware. Depending on demands
//! of virtualized systems, hypervisors may either provide transparent information from host machine
//! or provide another set of information to override the current environment. RISC-V hypervisors
//! does not have direct access to machine mode (M-mode) registers.
//!
//! RustSBI supports both by instance based providing a `MachineInfo` structure. If RISC-V
//! hypervisors choose to use existing information on current machine, it may require to call
//! underlying machine environment using SBI calls and fill in information into `MachineInfo`.
//! If hypervisors want to override hardware information, they may fill in custom ones into
//! `MachineInfo` structures. When creating RustSBI instance, `MachineInfo` structure is
//! required as an input of constructor.
//!
//! To begin with, disable default features in file `Cargo.toml`:
//!
//! ```toml
//! [dependencies]
//! rustsbi = { version = "0.3.0", default-features = false }
//! ```
//!
//! This will disable default feature `machine` which will assume that RustSBI runs on M-mode directly,
//! which is not appropriate in our purpose. After that, a `RustSBI` instance may be placed
//! in the virtual machine structure to prepare for SBI environment:
//!
//! ```rust
//! # struct RustSBI<>();
//! struct VmHart {
//!     // other fields ...
//!     env: RustSBI</* Types, .. */>,
//! }
//! ```
//!
//! When the virtual machine hart trapped into hypervisor, decide whether this trap is an SBI
//! environment call. If that is true, pass in parameters by `env.handle_ecall` function.
//! RustSBI will handle with SBI standard constants, call corresponding module and provide
//! parameters according to the extension and function IDs.
//!
//! Crate `rustsbi` adapts to standard RISC-V SBI calls.
//! If the hypervisor have custom SBI extensions that RustSBI does not recognize, those extension
//! and function IDs can be checked before calling RustSBI `env.handle_ecall`.
//!
//! ```no_run
//! # use sbi_spec::binary::SbiRet;
//! # struct MyExtensionEnv {}
//! # impl MyExtensionEnv { fn handle_ecall(&self, params: ()) -> SbiRet { SbiRet::success(0) } }
//! # struct RustSBI {} // Mock, prevent doc test error when feature singleton is enabled
//! # impl RustSBI { fn handle_ecall(&self, params: ()) -> SbiRet { SbiRet::success(0) } }
//! # struct VmHart { my_extension_env: MyExtensionEnv, env: RustSBI }
//! # #[derive(Copy, Clone)] enum Trap { Exception(Exception) }
//! # impl Trap { fn cause(&self) -> Self { *self } }
//! # #[derive(Copy, Clone)] enum Exception { SupervisorEcall }
//! # impl VmHart {
//! #     fn new() -> VmHart { VmHart { my_extension_env: MyExtensionEnv {}, env: RustSBI {} } }
//! #     fn run(&mut self) -> Trap { Trap::Exception(Exception::SupervisorEcall) }
//! #     fn trap_params(&self) -> () { }
//! #     fn fill_in(&mut self, ans: SbiRet) { let _ = ans; }
//! # }
//! let mut hart = VmHart::new();
//! loop {
//!     let trap = hart.run();
//!     if let Trap::Exception(Exception::SupervisorEcall) = trap.cause() {
//!         // Firstly, handle custom extensions
//!         let my_extension_sbiret = hart.my_extension_env.handle_ecall(hart.trap_params());
//!         // If custom extension handles correctly, fill in its result and continue to hart.
//!         // The custom handler may handle `probe_extension` in `base` extension as well
//!         // to allow detections to whether custom extension exists.
//!         if my_extension_sbiret != SbiRet::not_supported() {
//!             hart.fill_in(my_extension_sbiret);
//!             continue;
//!         }
//!         // Then, if it's not a custom extension, handle it using standard SBI handler.
//!         let standard_sbiret = hart.env.handle_ecall(hart.trap_params());
//!         hart.fill_in(standard_sbiret);
//!     }
//! }
//! ```
//!
//! RustSBI would interact well with custom extension environments in this way.
//!
//! ## Emulators using RustSBI
//!
//! RustSBI library may be used to write RISC-V emulators. Emulators do not use host hardware
//! features and thus may build and run on any architecture. Like hardware RISC-V implementations,
//! software emulated RISC-V environment would still need SBI implementation to support supervisor
//! environment.
//!
//! Writing emulators would follow the similiar process with writing hypervisors, see
//! [Hypervisors using RustSBI](#hypervisors-using-rustsbi) for details.
//!
//! # Download binary file: the Prototyping System vs discrete packages
//!
//! RustSBI ecosystem would typically provide support for most platforms. Those support packages
//! would be provided either from the RustSBI Prototyping System or vendor provided discrete SBI
//! implementation packages.
//!
//! The RustSBI Prototyping System is a universal support package provided by RustSBI ecosystem.
//! It is designed to save development time while providing most SBI feature possible.
//! Users may choose to download from Prototyping System repository to get various types of RustSBI
//! packages for their boards. Vendors and contributors may find it easy to adapt new SoCs and
//! boards into Prototyping System.
//!
//! Discrete SBI packages are SBI environment support packages specially designed for one board
//! or SoC, it will be provided by board vendor or RustSBI ecosystem.
//! Vendors may find it easy to include fine grained features in each support package, but the
//! maintainence situation would vary between vendors and it would likely to cost a lot of time
//! to develop from a bare-metal executable. Users may find a boost in performance, energy saving
//! indexes and feature granularity in discrete packages, but it would depends on whether the
//! vendor provide it.
//!
//! To download binary package for the Prototyping System, visit its project website for a download link.
//! To download them for discrete packages, RustSBI users may visit distribution source of SoC or board
//! manufacturers.
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
//! discovery methods (i.e. Device Tree or ACPI) for that purpose.
//!
//! To detect any feature under bare metal or under supervisor level, developers may depend on
//! any hardware discovery methods, or use try-execute-trap method to detect any instructions or
//! CSRs. If SBI is implemented in user level emulators, it may requires to depend on operating
//! system calls or use the signal trap method to detect any RISC-V core features.
//!
//! # Notes for RustSBI developers
//!
//! Following useful hints are for firmware and kernel developers when working with SBI and RustSBI.
//!
//! ## RustSBI is a library for interfaces
//!
//! This library adapts to individual Rust traits to provide basic SBI features.
//! When building for own platform, implement traits in this library and pass them to the functions
//! begin with `init`. After that, you may call `rustsbi::ecall`, `RustSBI::handle_ecall` or
//! similiar functions in your own exception handler.
//! It would dispatch parameters from supervisor to the traits to execute SBI functions.
//!
//! The library also implements useful functions which may help with platform specific binaries.
//! The `LOGO` can be printed if necessary when the binary is initializing.
//!
//! Note that this crate is a library which contains common building blocks in SBI implementation.
//! The RustSBI ecosystem would provide different level of support for each board, those support
//! packages would use `rustsbi` crate as library to provide different type of SBI binary releases.
//!
//! ## Legacy SBI extension
//!
//! *Note: RustSBI legacy support is only designed for backward compability of RISC-V SBI standard.
//! It's disabled by default and it's not suggested to include legacy functions in newer firmware designs.
//! Modules other than legacy console is replaced by individual modules in SBI.
//! Kernels are not suggested to use legacy functions in practice.
//! If you are a kernel developer, newer designs should consider relying on each SBI module other than
//! legacy functions.*
//!
//! The SBI includes legacy extension which dated back to SBI 0.1 specification. Most of its features
//! are replaced by individual SBI modules, thus the entire legacy extension is deprecated by
//! SBI version 0.2. However, some users may find out SBI 0.1 legacy console useful in some situations
//! even if it's deprecated.
//!
//! RustSBI keeps SBI 0.1 legacy support under feature gate `legacy`. To use RustSBI with legacy feature,
//! you may change dependency code to:
//!
//! ```toml
//! [dependencies]
//! rustsbi = { version = "0.3.0", features = ["legacy"] }
//! ```

#![no_std]
#![cfg_attr(feature = "singleton", feature(ptr_metadata))]

#[cfg(feature = "legacy")]
#[doc(hidden)]
#[macro_use]
pub mod legacy_stdio;
mod base;
#[cfg(feature = "singleton")]
mod ecall;
mod hart_mask;
mod hsm;
#[cfg(not(feature = "legacy"))]
mod instance;
mod ipi;
mod pmu;
mod reset;
mod rfence;
mod timer;
#[cfg(feature = "singleton")]
mod util;

/// The RustSBI logo without blank lines on the beginning
pub const LOGO: &str = r".______       __    __      _______.___________.  _______..______   __
|   _  \     |  |  |  |    /       |           | /       ||   _  \ |  |
|  |_)  |    |  |  |  |   |   (----`---|  |----`|   (----`|  |_)  ||  |
|      /     |  |  |  |    \   \       |  |      \   \    |   _  < |  |
|  |\  \----.|  `--'  |.----)   |      |  |  .----)   |   |  |_)  ||  |
| _| `._____| \______/ |_______/       |__|  |_______/    |______/ |__|";

const SBI_SPEC_MAJOR: usize = 1;
const SBI_SPEC_MINOR: usize = 0;

/// RustSBI implementation ID: 4
///
/// Ref: https://github.com/riscv-non-isa/riscv-sbi-doc/pull/61
const IMPL_ID_RUSTSBI: usize = 4;

const RUSTSBI_VERSION_MAJOR: usize = (env!("CARGO_PKG_VERSION_MAJOR").as_bytes()[0] - b'0') as _;
const RUSTSBI_VERSION_MINOR: usize = (env!("CARGO_PKG_VERSION_MINOR").as_bytes()[0] - b'0') as _;
const RUSTSBI_VERSION_PATCH: usize = (env!("CARGO_PKG_VERSION_PATCH").as_bytes()[0] - b'0') as _;
const RUSTSBI_VERSION: usize =
    (RUSTSBI_VERSION_MAJOR << 16) + (RUSTSBI_VERSION_MINOR << 8) + RUSTSBI_VERSION_PATCH;

/// RustSBI version as a string
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub extern crate sbi_spec as spec;
#[cfg(feature = "singleton")]
pub use ecall::handle_ecall as ecall;
pub use hart_mask::HartMask;
pub use hsm::Hsm;
#[cfg(not(feature = "legacy"))]
pub use instance::{Builder, RustSBI};
pub use ipi::Ipi;
#[cfg(feature = "legacy")]
#[doc(hidden)]
pub use legacy_stdio::{legacy_stdio_getchar, legacy_stdio_putchar};
pub use pmu::Pmu;
pub use reset::Reset;
pub use rfence::Rfence as Fence;
pub use timer::Timer;

#[cfg(not(feature = "machine"))]
pub use instance::MachineInfo;

#[cfg(feature = "singleton")]
pub use {
    hsm::init_hsm, ipi::init_ipi, pmu::init_pmu, reset::init_reset,
    rfence::init_rfence as init_remote_fence, timer::init_timer,
};
