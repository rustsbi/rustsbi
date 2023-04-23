//! A minimal RISC-V's SBI implementation library in Rust.
//!
//! *Note: If you are a user looking for binary distribution download for RustSBI, you may consider
//! using the [RustSBI Prototyping System](https://github.com/rustsbi/standalone)
//! which will provide binaries for each platforms.
//! If you are a vendor or contributor who wants to adapt RustSBI to your new product or board,
//! you may consider adapting the Prototyping System first to get your board adapted in an afternoon;
//! you are only advised to build a discrete crate if your team have a lot of time working on this board.*
//!
//! *For more details on binary downloads the the RustSBI Prototyping System,
//! see section [Prototyping System vs discrete packages](#download-binary-file-the-prototyping-system-vs-discrete-packages).*
//!
//! The crate `rustsbi` acts as core trait and instance abstraction of the RustSBI ecosystem.
//!
//! # What is RISC-V SBI?
//!
//! RISC-V SBI is short for RISC-V Supervisor Binary Interface. SBI acts as an interface to environment
//! for your operating system kernel.
//! An SBI implementation will allow furtherly bootstrap your kernel, and provide an environment while the kernel is running.
//!
//! More generally, The SBI allows supervisor-mode (S-mode or VS-mode) software to be portable across
//! all RISC-V implementations by defining an abstraction for platform (or hypervisor) specific functionality.
//!
//! # Use RustSBI services in your supervisor software
//!
//! SBI environment features include boot sequence and a kernel environment. To bootstrap your kernel,
//! place kernel into RustSBI implementation defined address, then RustSBI will prepare an
//! environment and call the entry function on this address.
//!
//! ## Make SBI environment calls
//!
//! To use the kernel environment, you either use SBI calls or emulated instructions.
//! SBI calls are similar to operating systems' `syscall`s. RISC-V SBI defined many SBI extensions,
//! and in each extension there are different functions, you should pick a function before calling.
//! Then, you should prepare some parameters, whose definition are not the same among functions.
//!
//! Now you have an extension number, a function number, and a few SBI call parameters.
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
//! Extension number is required to put on register `a7`, function number on `a6` if applicable.
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
//! SBI functions would return a result thus some of these may fail.
//! In this example we only take the value, but in complete designs we should handle the `error`
//! returned by SbiRet.
//!
//! You may use other languages to call SBI environment. In C programming language, we can call like this:
//!
//! ```text
//! #define SBI_CALL(ext, funct, arg0, arg1, arg2, arg3) ({ \
//!     register uintptr_t a0 asm ("a0") = (uintptr_t)(arg0); \
//!     register uintptr_t a1 asm ("a1") = (uintptr_t)(arg1); \
//!     register uintptr_t a2 asm ("a2") = (uintptr_t)(arg2); \
//!     register uintptr_t a3 asm ("a3") = (uintptr_t)(arg3); \
//!     register uintptr_t a6 asm ("a6") = (uintptr_t)(funct); \
//!     register uintptr_t a7 asm ("a7") = (uintptr_t)(ext); \
//!     asm volatile ("ecall" \
//!         : "+r" (a0), "+r" (a1) \
//!         : "r" (a1), "r" (a2), "r" (a3), "r" (a6), "r" (a7) \
//!         : "memory") \
//!     {a0, a1}; \
//! })
//!
//! #define SBI_CALL_0(ext, funct) SBI_CALL(ext, funct, 0, 0, 0, 0)
//!
//! static inline sbiret get_spec_version() {
//!     SBI_CALL_0(EXTENSION_BASE, FUNCTION_BASE_GET_SPEC_VERSION)
//! }
//! ```
//!
//! # Implement RustSBI on machine environment
//!
//! Boards, SoC vendors, machine environment emulators and research projects may adapt RustSBI
//! to specific environments.
//! RustSBI project supports these demands either by discrete package or the Prototyping System.
//! Developers may choose the Prototyping System to shorten development time,
//! or discrete packages to include fine-grained features.
//!
//! Hypervisor and supervisor environment emulator developers may refer to
//! [Hypervisor and emulator development with RustSBI](#hypervisor-and-emulator-development-with-rustsbi)
//! for such purposes as RustSBI provide different set of features dedicated for emulated or virtual
//! environments.
//!
//! ## Use the Prototyping System
//!
//! The RustSBI Prototyping System aims to get your platform working with SBI in an afternoon.
//! It supports most RISC-V platforms available by providing scalable set of drivers and features.
//! It provides custom features such as Penglai TEE, DramForever's emulated hypervisor extension, and Raven
//! the firmware debugger framework.
//!
//! You may find further documents on [RustSBI Prototyping System repository](https://github.com/rustsbi/standalone).
//!
//! ## Discrete RustSBI package on bare metal RISC-V hardware
//!
//! Discrete packages provide developers with most scalability and complete control of underlying
//! hardware. It is ideal if advanced low power features, management cores and other features should
//! be used in this implementation.
//!
//! RustSBI supports discrete package by default. Create a new `#![no_std]` bare-metal package
//! to get started. Add following lines to `Cargo.toml`:
//!
//! ```toml
//! [dependencies]
//! rustsbi = { version = "0.4.0", features = ["machine"] }
//! ```
//!
//! The feature `machine` indicates that RustSBI library is run directly on machine mode RISC-V
//! environment; it will use `riscv` crate to fetch machine mode environment, which fits our demand
//! of using it on bare metal RISC-V.
//!
//! After hardware initialization process, the part of firmware with RustSBI linked should run on memory
//! blocks with fast accesses, as it would be called frequently by operating system.
//! If the supervisor is called by trap generator semantics, insert `rustsbi::RustSBI` structure
//! in your hart executor structure.
//!
//! ```rust
//! # struct Clint;
//! # struct MyPlatRfnc;
//! # struct MyPlatHsm;
//! # struct MyBoardPower;
//! # struct MyPlatPmu;
//! # struct MyPlatDbcn;
//! # struct MyPlatSusp;
//! # struct MyPlatCppc;
//! use rustsbi::RustSBI;
//!
//! # struct SupervisorContext;
//! /// Executes the supervisor within.
//! struct Executor {
//!     ctx: SupervisorContext,
//!     /* other environment variables ... */
//!     sbi: RustSBI<Clint, Clint, MyPlatRfnc, MyPlatHsm, MyBoardPower, MyPlatPmu, MyPlatDbcn, MyPlatSusp, MyPlatCppc>,
//!     /* custom_1: CustomSBI<...> */
//! }
//!
//! # struct Trap;
//! impl Executor {
//!     /// A function that runs the provided supervisor, uses `&mut self` for it
//!     /// modifies `SupervisorContext`.
//!     ///
//!     /// It returns for every Trap the supervisor produces. Its handler should read
//!     /// and modify `self.ctx` if necessary. After handled, `run()` this structure
//!     /// again or exit execution process.
//!     pub fn run(&mut self) -> Trap {
//!         todo!("fill in generic or platform specific trampoline procedure")
//!     }
//! }
//! ```
//!
//! After each `run()`, process the trap returned with the RustSBI instance in executor.
//! Call `RustSBI::handle_ecall` and fill in developer provided `SupervisorContext` if necessary.
//!
//! ```no_run
//! # use sbi_spec::binary::SbiRet;
//! # struct RustSBI {} // Mock, prevent doc test error when feature singleton is enabled
//! # impl RustSBI { fn handle_ecall(&self, e: (), f: (), p: ()) -> SbiRet { SbiRet::success(0) } }
//! # struct Executor { sbi: RustSBI }
//! # #[derive(Copy, Clone)] enum Trap { Exception(Exception) }
//! # impl Trap { fn cause(&self) -> Self { *self } }
//! # #[derive(Copy, Clone)] enum Exception { SupervisorEcall }
//! # impl Executor {
//! #     fn new(board_params: BoardParams) -> Executor { let _ = board_params; Executor { sbi: RustSBI {} } }
//! #     fn run(&mut self) -> Trap { Trap::Exception(Exception::SupervisorEcall) }
//! #     fn sbi_extension(&self) -> () { }
//! #     fn sbi_function(&self) -> () { }
//! #     fn sbi_params(&self) -> () { }
//! #     fn fill_sbi_return(&mut self, ans: SbiRet) { let _ = ans; }
//! # }
//! # struct BoardParams;
//! # const MY_SPECIAL_EXIT: usize = 0x233;
//! /// Board specific power operations.
//! enum Operation {
//!     Reboot,
//!     Shutdown,
//! }
//!
//! # impl From<SbiRet> for Operation { fn from(_: SbiRet) -> Self { todo!() } }
//! /// Execute supervisor in given board parameters.
//! pub fn execute_supervisor(board_params: BoardParams) -> Operation {
//!     let mut exec = Executor::new(board_params);
//!     loop {
//!         let trap = exec.run();
//!         if let Trap::Exception(Exception::SupervisorEcall) = trap.cause() {
//!             let ans = exec.sbi.handle_ecall(
//!                 exec.sbi_extension(),
//!                 exec.sbi_function(),
//!                 exec.sbi_params(),
//!             );
//!             if ans.error == MY_SPECIAL_EXIT {
//!                 break Operation::from(ans)
//!             }
//!             // This line would also advance `sepc` with `4` to indicate the `ecall` is handled.
//!             exec.fill_sbi_return(ans);
//!         } else {
//!             // other trap types ...
//!         }
//!     }
//! }
//! ```
//!
//! Now, call supervisor execution function in your bare metal package to finish the discrete
//! package project.
//!
//! ```no_run
//! # #[cfg(nightly)] // disable checks
//! #[naked]
//! #[link_section = ".text.entry"]
//! #[export_name = "_start"]
//! unsafe extern "C" fn entry() -> ! {
//!     #[link_section = ".bss.uninit"]
//!     static mut SBI_STACK: [u8; LEN_STACK_SBI] = [0; LEN_STACK_SBI];
//!
//!     // Note: actual assembly code varies between platforms.
//!     // Double check documents before continue on.
//!     core::arch::asm!(
//!         // 1. Turn off interrupt
//!         "csrw  mie, zero",
//!         // 2. Initialize programming langauge runtime
//!         // only clear bss if hartid is zero
//!         "csrr  t0, mhartid",
//!         "bnez  t0, 2f",
//!         // clear bss segment
//!         "la  t0, sbss",
//!         "la  t1, ebss",
//!         "1:",
//!         "bgeu  t0, t1, 2f",
//!         "sd  zero, 0(t0)",
//!         "addi  t0, t0, 8",
//!         "j  1b",
//!         "2:",
//!         // 3. Prepare stack for each hart
//!         "la  sp, {stack}",
//!         "li  t0, {per_hart_stack_size}",
//!         "csrr  t1, mhartid",
//!         "addi  t1, t1, 1",
//!         "1: ",
//!         "add  sp, sp, t0",
//!         "addi  t1, t1, -1",
//!         "bnez  t1, 1b",
//!         "j  {rust_main}",
//!         // 4. Clean up
//!         "j  {finalize}",
//!         per_hart_stack_size = const LEN_STACK_PER_HART,
//!         stack = sym SBI_STACK,
//!         rust_main = sym rust_main,
//!         finalize = sym finalize,
//!         options(noreturn)
//!     )
//! }
//!
//! # fn board_init_once() {}
//! # fn print_information_once() {}
//! # fn execute_supervisor(_bp: &()) -> Operation { Operation::Shutdown }
//! /// Power operation after main function
//! enum Operation {
//!     Reboot,
//!     Shutdown,
//!     // Add board specific low power modes if necessary. This will allow the
//!     // function `finalize` to operate on board specific power management chips.
//! }
//!
//! /// Rust entry, call in `entry` assembly function
//! extern "C" fn rust_main(_hartid: usize, opaque: usize) -> Operation {
//!     // .. board initialization process ...
//!     let board_params = board_init_once();
//!     // .. print necessary information and rustsbi::LOGO ..
//!     print_information_once();
//!     // execute supervisor, return as Operation
//!     execute_supervisor(&board_params)
//! }
//!
//! # fn wfi() {}
//! /// Perform board specific power operations
//! ///
//! /// The function here provides a stub to example power operations.
//! /// Actual board developers should provide with more practical communications
//! /// to external chips on power operation.
//! unsafe extern "C" fn finalize(op: Operation) -> ! {
//!     match op {
//!         Operation::Shutdown => {
//!             // easiest way to make a hart look like powered off
//!             loop { wfi(); }
//!         }
//!         Operation::Reboot => {
//! # fn entry() -> ! { loop {} } // mock
//!             // easiest software reset is to jump to entry directly
//!             entry()
//!         }
//!         // .. more power operations goes here ..
//!     }
//! }
//! ```
//!
//! Now RustSBI would run on machine environment, you may start a kernel or use an SBI test suite
//! to check if it is properly implemented.
//!
//! Some platforms would provide system memory under different grades in speed and size to reduce product cost.
//! Those platforms would typically provide two parts of code memory, first one being relatively small, not fast
//! but instantly available after chip start, while the second one is larger in size but typically requires
//! memory training. The former one would include built-in SRAM memory, and the later would include
//! external SRAM or DDR memory. On those platforms, a first stage bootloader is typically needed to
//! train memory for later stages. In such situation, RustSBI implementation should be linked or concated
//! to the second stage bootloader, and the first stage could be a standalone binary package bundled with it.
//!
//! # Hypervisor and emulator development with RustSBI
//!
//! RustSBI crate supports to develop RISC-V emulators, and both Type-1 and Type-2 hypervisors.
//! Hypervisor developers may find it easy to handle standard SBI functions with an instance
//! based RustSBI interface.
//!
//! ## Hypervisors using RustSBI
//!
//! Both Type-1 and Type-2 hypervisors on RISC-V run on HS-mode hardware. Depending on demands
//! of virtualized systems, hypervisors may either provide transparent information from host machine
//! or provide another set of information to override the current environment. Notably,
//! RISC-V hypervisors do not have direct access to machine mode (M-mode) registers.
//!
//! RustSBI supports both by providing a `MachineInfo` structure in instance based interface.
//! If RISC-V hypervisors choose to use existing information on current machine, it may require
//! to call underlying M-mode environment using SBI calls and fill in information into `MachineInfo`.
//! If hypervisors use customized information other than taking the same one from the
//! environment they reside in, they may fill in custom one into `MachineInfo` structures.
//! When creating RustSBI instance, `MachineInfo` structure is required as an input of constructor.
//!
//! To begin with, include RustSBI library in file `Cargo.toml`:
//!
//! ```toml
//! [dependencies]
//! rustsbi = "0.4.0"
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
//! When the virtual machine hart traps into hypervisor, its code should decide whether
//! this trap is an SBI environment call. If that is true, pass in parameters by `env.handle_ecall`
//! function. RustSBI will handle with SBI standard constants, call corresponding extension module
//! and provide parameters according to the extension and function IDs.
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
//! RustSBI library may be used to write RISC-V emulators. Other than hardware accelereted binary
//! translation methods, emulators typically do not use host hardware specific features,
//! thus may build and run on any architecture.
//! Like hardware RISC-V implementations, software emulated RISC-V environment would still need SBI
//! implementation to support supervisor environment.
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
//! It also includes a universal test kernel to allow testing SBI implementations on current environment.
//! Users may choose to download from [Prototyping System repository](https://github.com/rustsbi/standalone)
//! to get various types of RustSBI packages for their boards.
//! Vendors and contributors may find it easy to adapt new SoCs and boards into Prototyping System.
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
//! manufacturers. Additionally, users may visit [the awesome page](https://github.com/rustsbi/awesome-rustsbi)
//! for a curated list ofboth Prototyping System and discrete packages provided by RustSBI ecosystem.
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
//! ## Hardware discovery and feature detection
//!
//! According to the RISC-V SBI specification, SBI itself does not specify any method for hardware discovery.
//! The supervisor software must rely on the other industry standard hardware
//! discovery methods (i.e. Device Tree or ACPI) for that purpose.
//!
//! To detect any feature under bare metal or under supervisor level, developers may depend on
//! any hardware discovery methods, or use try-execute-trap method to detect any instructions or
//! CSRs. If SBI is implemented in user level emulators, it may requires to depend on operating
//! system calls or use the signal trap method to detect any RISC-V core features.

#![no_std]

mod console;
mod cppc;
mod hart_mask;
mod hsm;
mod instance;
mod ipi;
mod pmu;
mod reset;
mod rfence;
mod susp;
mod timer;

/// The RustSBI logo without blank lines on the beginning
pub const LOGO: &str = r".______       __    __      _______.___________.  _______..______   __
|   _  \     |  |  |  |    /       |           | /       ||   _  \ |  |
|  |_)  |    |  |  |  |   |   (----`---|  |----`|   (----`|  |_)  ||  |
|      /     |  |  |  |    \   \       |  |      \   \    |   _  < |  |
|  |\  \----.|  `--'  |.----)   |      |  |  .----)   |   |  |_)  ||  |
| _| `._____| \______/ |_______/       |__|  |_______/    |______/ |__|";

// RustSBI supports RISC-V SBI specification 2.0-rc1
const SBI_SPEC_MAJOR: usize = 2;
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
pub use console::Console;
pub use cppc::Cppc;
pub use hart_mask::HartMask;
pub use hsm::Hsm;
pub use instance::{Builder, RustSBI};
pub use ipi::Ipi;
pub use pmu::Pmu;
pub use reset::Reset;
pub use rfence::Rfence as Fence;
pub use susp::Susp;
pub use timer::Timer;

#[cfg(not(feature = "machine"))]
pub use instance::MachineInfo;
