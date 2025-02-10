//! A minimal RISC-V's SBI implementation library in Rust.
//!
//! *Note: If you are a user looking for binary distribution download for RustSBI, you may consider
//! using the [RustSBI Prototyper](https://github.com/rustsbi/prototyper)
//! which will provide binaries for each platform.
//! If you are a vendor or contributor who wants to adapt RustSBI to your new product or board,
//! you may consider adapting the Prototyper first to get your board adapted in a short period of time;
//! or build a discrete crate if your team has plenty of time working on this board.*
//!
//! *For more details on binary downloads the RustSBI Prototyper,
//! see section [Prototyper vs. discrete packages](#download-binary-file-the-prototyper-vs-discrete-packages).*
//!
//! The crate `rustsbi` acts as core trait, extension abstraction and implementation generator
//! of the RustSBI ecosystem.
//!
//! # What is RISC-V SBI?
//!
//! RISC-V SBI is short for RISC-V Supervisor Binary Interface. SBI acts as an interface to environment
//! for the operating system kernel.
//! An SBI implementation will allow further bootstrapping the kernel and provide a supportive environment
//! while the kernel is running.
//!
//! More generally, The SBI allows supervisor-mode (S-mode or VS-mode) software to be portable across
//! all RISC-V implementations by defining an abstraction for platform (or hypervisor) specific functionality.
//!
//! # Use RustSBI services in the supervisor software
//!
//! SBI environment features include boot sequence and an S-mode environment. To bootstrap the
//! S-mode software, the kernel (or other supervisor-level software) would be loaded
//! into an implementation-defined address, then RustSBI will prepare an environment and
//! enter the S-mode software on the S-mode visible harts. If the firmware environment provides
//! other boot-loading standards upon SBI, following bootstrap process will provide further
//! information on the supervisor software.
//!
//! ## Make SBI environment calls
//!
//! To use the underlying environment, the supervisor either uses SBI calls or run software
//! implemented instructions.
//! SBI calls are similar to the system calls for operating systems. The SBI extensions, whether
//! defined by the RISC-V SBI Specification or by custom vendors, would either consume parameters
//! only, or defined a list of functions identified by function IDs, where the S-mode software
//! would pick and call. Definition of parameters varies between extensions and functions.
//!
//! At this point, we have picked up an extension ID, a function ID, and a few SBI call parameters.
//! Now instead of a conventional jump instruction, the software would invoke a special `ecall`
//! instruction on supervisor level to transfer the control flow, resulting into a trap to the SBI
//! environment. The SBI environment will process the `ecall` and fill in SBI call results,
//! similar to what an operating system would handle system calls from user level.
//!
//! All SBI calls would return two integers: the error number and the return value.
//! The error number will tell if the SBI call has been successfully proceeded, or which error
//! has occurred. The return value indicates the result of a successful SBI call, whose
//! meaning is different among different SBI extensions.
//!
//! ## Call SBI in Rust or other programming languages
//!
//! Making SBI calls is similar to making system calls; RISC-V SBI calls pass extension ID,
//! function ID (if applicable) and parameters in integer registers.
//!
//! The extension ID is required to put on register `a7`, function ID on `a6` if applicable.
//! Parameters should be placed from `a0` to `a5`, first into `a0`, second into `a1`, etc.
//! Unused parameters can be set to any value or leave untouched.
//!
//! After registers are ready, the S-mode software would invoke an `ecall` instruction.
//! The SBI call will return two values placed in `a0` and `a1` registers;
//! the error value could be read from `a0`, and the return value is placed into `a1`.
//!
//! In Rust, we would usually use crates like [`sbi-rt`](https://crates.io/crates/sbi-rt)
//! to hide implementation details and focus on supervisor software development.
//! However, if in some cases we have to write them in inline assembly, here is an example
//! to do this:
//!
//! ```no_run
//! # #[repr(C)] struct SbiRet { error: usize, value: usize }
//! # const EXTENSION_BASE: usize = 0x10;
//! # const FUNCTION_BASE_GET_SPEC_VERSION: usize = 0x0;
//! #[inline(always)]
//! fn sbi_call_2(extension: usize, function: usize, arg0: usize, arg1: usize) -> SbiRet {
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
//!             let _ = (extension, function, arg0, arg1);
//!             unimplemented!("not RISC-V instruction set architecture")
//!         }
//!     };
//!     SbiRet { error, value }
//! }
//!
//! #[inline]
//! pub fn get_spec_version() -> SbiRet {
//!     sbi_call_2(EXTENSION_BASE, FUNCTION_BASE_GET_SPEC_VERSION, 0, 0)
//! }
//! ```
//!
//! SBI calls may fail, returning the corresponding type of error in an error code.
//! In this example, we only take the value, but in complete designs we should handle
//! the `error` code returned by SbiRet thoroughly.
//!
//! In other programming languages, similar methods may be achieved by inline assembly
//! or other features; its documentation may suggest which is the best way to achieve this.
//!
//! # Implement RustSBI on machine environment
//!
//! Boards, SoC vendors, machine environment emulators and research projects may adapt RustSBI
//! to specific environments.
//! RustSBI project supports these demands either by discrete package or the Prototyper.
//! Developers may choose the Prototyper to shorten development time,
//! or discrete packages to include fine-grained features.
//!
//! Hypervisor and supervisor environment emulator developers may refer to
//! [Hypervisor and emulator development with RustSBI](#hypervisor-and-emulator-development-with-rustsbi)
//! for such purposes, as RustSBI provides a different set of features dedicated to emulated or virtual
//! environments.
//!
//! ## Use the Prototyper
//!
//! The RustSBI Prototyper aims to get your platform working with SBI in a short period of time.
//! It supports most RISC-V platforms available by providing a scalable set of drivers and features.
//! It provides useful custom features such as Penglai TEE, DramForever's emulated hypervisor extension,
//! and Raven the firmware debugger framework.
//!
//! You may find further documents on [RustSBI Prototyper repository](https://github.com/rustsbi/prototyper).
//!
//! ## Discrete RustSBI package on bare metal RISC-V hardware
//!
//! Discrete packages provide developers with most scalability and complete control of underlying
//! hardware. It is ideal if detailed SoC low-power features, management cores and other features
//! would be used in the SBI implementation.
//!
//! RustSBI supports discrete package development out-of-box. If we are running on bare-metal, we
//! can create a new `#![no_std]` bare-metal package, add runtime code or use runtime libraries
//! to get started. Then, we add the following lines to `Cargo.toml`:
//!
//! ```toml
//! [dependencies]
//! rustsbi = { version = "0.4.0", features = ["machine"] }
//! ```
//!
//! The feature `machine` indicates that RustSBI library is run directly on machine mode RISC-V
//! environment; it will use the `riscv` crate to fetch machine mode environment information by CSR
//! instructions, which fits our demand of using it on bare metal RISC-V.
//!
//! After hardware initialization process, the part of firmware with RustSBI linked should run on
//! memory blocks with fast access, as it would be called frequently by operating system.
//! If the implementation treats the supervisor as a generator of traps, we insert `rustsbi::RustSBI`
//! implementation in a hart executor structure.
//!
//! ```rust
//! use rustsbi::RustSBI;
//!
//! # struct SupervisorContext;
//! /// Executes the supervisor within.
//! struct Executor {
//!     ctx: SupervisorContext,
//!     /* other environment variables ... */
//!     sbi: MySBI,
//!     /* custom_1: CustomSBI, ... */
//! }
//!
//! #[derive(RustSBI)]
//! struct MySBI {
//!     console: MyConsole,
//!     // todo: other extensions ...
//!     info: MyEnvInfo,
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
//! # use sbi_spec::binary::{SbiRet, Physical};
//! # struct MyConsole;
//! # impl rustsbi::Console for MyConsole {
//! #     fn write(&self, _: Physical<&[u8]>) -> SbiRet { unimplemented!() }
//! #     fn read(&self, _: Physical<&mut [u8]>) -> SbiRet { unimplemented!() }
//! #     fn write_byte(&self, _: u8) -> SbiRet { unimplemented!() }
//! # }
//! # struct MyEnvInfo;
//! # impl rustsbi::EnvInfo for MyEnvInfo {
//! #     fn mvendorid(&self) -> usize { 1 }
//! #     fn marchid(&self) -> usize { 2 }
//! #     fn mimpid(&self) -> usize { 3 }
//! # }
//! ```
//!
//! After each `run()`, the SBI implementation would process the trap returned with the RustSBI
//! instance in executor. Call the function `handle_ecall` (generated by the derive macro `RustSBI`)
//! and fill in a `SupervisorContext` if necessary.
//!
//! ```no_run
//! # use rustsbi::RustSBI;
//! # use sbi_spec::binary::{SbiRet, HartMask};
//! # struct MyEnvInfo;
//! # impl rustsbi::EnvInfo for MyEnvInfo {
//! #     fn mvendorid(&self) -> usize { 1 }
//! #     fn marchid(&self) -> usize { 2 }
//! #     fn mimpid(&self) -> usize { 3 }
//! # }
//! # #[derive(RustSBI)]
//! # struct MySBI { info: MyEnvInfo } // extensions ...
//! # struct Executor { sbi: MySBI }
//! # #[derive(Copy, Clone)] enum Trap { Exception(Exception) }
//! # impl Trap { fn cause(&self) -> Self { *self } }
//! # #[derive(Copy, Clone)] enum Exception { SupervisorEcall }
//! # impl Executor {
//! #     fn new(board_params: BoardParams) -> Executor { let _ = board_params; Executor { sbi: MySBI { info: MyEnvInfo } } }
//! #     fn run(&mut self) -> Trap { Trap::Exception(Exception::SupervisorEcall) }
//! #     fn sbi_extension(&self) -> usize { unimplemented!() }
//! #     fn sbi_function(&self) -> usize { unimplemented!() }
//! #     fn sbi_params(&self) -> [usize; 6] { unimplemented!() }
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
//!         }
//!         // else {
//!         //    // other trap types ...
//!         // }
//!     }
//! }
//! ```
//!
//! Now, call the supervisor execution function in your bare metal package to finish the discrete
//! package project. Here is an example of a bare-metal entry; actual projects would either
//! use a library for runtime, or write assemble code only if necessary.
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
//!     // Double-check documents before continue on.
//!     core::arch::asm!(
//!         // 1. Turn off interrupt
//!         "csrw  mie, zero",
//!         // 2. Initialize programming language runtime
//!         // only clear bss if hart ID is zero
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
//! /// Power operation after main function.
//! enum Operation {
//!     Reboot,
//!     Shutdown,
//!     // Add board-specific low-power modes if necessary. This will allow the
//!     // function `finalize` to operate on board-specific power management chips.
//! }
//!
//! /// Rust entry, call in `entry` assembly function
//! extern "C" fn rust_main(_hart_id: usize, opaque: usize) -> Operation {
//!     // board initialization process
//!     let board_params = board_init_once();
//!     // print necessary information and rustsbi::LOGO
//!     print_information_once();
//!     // execute supervisor, return as Operation
//!     execute_supervisor(&board_params)
//! }
//!
//! # fn wfi() {}
//! /// Perform board-specific power operations.
//! ///
//! /// The function here provides a stub to example power operations.
//! /// Actual board developers should provide more practical communications
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
//!         // more power operations go here
//!     }
//! }
//! ```
//!
//! Now RustSBI would run on machine environment, a kernel may be started or use an SBI test suite
//! to check if it is properly implemented.
//!
//! Some platforms would provide system memory under different grades in speed and size to reduce product cost.
//! Those platforms would typically provide two parts of code memory, the first one being relatively small, not fast
//! but instantly available after chip start, while the second one is larger but typically requires
//! memory training. The former one would include built-in SRAM memory, and the later would include
//! external SRAM or DDR memory. On those platforms, a first stage bootloader is typically needed to
//! train memory for later stages. In such a situation, RustSBI implementation should be treated as or concatenated
//! to the second stage bootloader, and the first stage could be a standalone binary package bundled with it.
//!
//! # Hypervisor and emulator development with RustSBI
//!
//! RustSBI crate supports developing RISC-V emulators, and both Type-1 and Type-2 hypervisors.
//! Hypervisor developers may find it easy to handle standard SBI functions with an instance-based RustSBI interface.
//!
//! ## Hypervisors using RustSBI
//!
//! Both Type-1 and Type-2 hypervisors on RISC-V run on HS-mode hardware. Depending on the demands
//! of virtualized systems, hypervisors may either provide transparent information from the host machine
//! or provide another set of information to override the current environment. Notably,
//! RISC-V hypervisors do not have direct access to machine mode (M-mode) registers.
//!
//! RustSBI supports both by accepting an implementation of the `EnvInfo` trait.
//! If RISC-V hypervisors choose to use existing information on the current machine,
//! it may require calling underlying M-mode environment using SBI calls and fill in information
//! into the variable implementing trait `EnvInfo`.
//! If hypervisors use customized information other than taking the same one from the
//! environment they reside in, they may build custom structures implementing `EnvInfo` to provide
//! customized machine information.
//! Deriving a RustSBI instance without bare-metal support would require an `EnvInfo` implementation
//! as an input of the derive-macro.
//!
//! To begin with, include the RustSBI library in file `Cargo.toml`:
//!
//! ```toml
//! [dependencies]
//! rustsbi = "0.4.0"
//! ```
//!
//! This will disable the default feature `machine` which will assume that RustSBI runs on M-mode directly,
//! which is not appropriate for our purpose. After that, define an SBI structure and derive its `RustSBI`
//! implementation using `#[derive(RustSBI)]`. The defined SBI structure can be placed in
//! a virtual machine structure representing a control flow executor to prepare for SBI environment:
//!
//! ```rust
//! use rustsbi::RustSBI;
//!
//! #[derive(RustSBI)]
//! struct MySBI {
//!     // add other fields later ...
//!     // The environment information must be provided on
//!     // non-bare-metal RustSBI development.
//!     info: MyEnvInfo,
//! }
//!
//! struct VmHart {
//!     // other fields ...
//!     sbi: MySBI,
//! }
//! # struct MyEnvInfo;
//! # impl rustsbi::EnvInfo for MyEnvInfo {
//! #     fn mvendorid(&self) -> usize { 1 }
//! #     fn marchid(&self) -> usize { 2 }
//! #     fn mimpid(&self) -> usize { 3 }
//! # }
//! ```
//!
//! When the virtual machine hart traps into hypervisor, its code should decide whether
//! this trap is an SBI environment call. If that is true, pass in parameters by `sbi.handle_ecall`
//! function. RustSBI will handle with SBI standard constants, call the corresponding extension field
//! and provide parameters according to the extension and function IDs (if applicable).
//!
//! Crate `rustsbi` adapts to standard RISC-V SBI calls.
//! If the hypervisor has custom SBI extensions that RustSBI does not recognize, those extension
//! and function IDs can be checked before calling RustSBI `env.handle_ecall`.
//!
//! ```no_run
//! # use sbi_spec::binary::{SbiRet, HartMask};
//! # struct MyExtensionSBI {}
//! # impl MyExtensionSBI { fn handle_ecall(&self, params: ()) -> SbiRet { SbiRet::success(0) } }
//! # struct MySBI {} // Mock, prevent doc test error when feature singleton is enabled
//! # impl MySBI { fn handle_ecall(&self, params: ()) -> SbiRet { SbiRet::success(0) } }
//! # struct VmHart { my_extension_sbi: MyExtensionSBI, sbi: MySBI }
//! # #[derive(Copy, Clone)] enum Trap { Exception(Exception) }
//! # impl Trap { fn cause(&self) -> Self { *self } }
//! # #[derive(Copy, Clone)] enum Exception { SupervisorEcall }
//! # impl VmHart {
//! #     fn new() -> VmHart { VmHart { my_extension_sbi: MyExtensionSBI {}, sbi: MySBI {} } }
//! #     fn run(&mut self) -> Trap { Trap::Exception(Exception::SupervisorEcall) }
//! #     fn trap_params(&self) -> () { }
//! #     fn fill_in(&mut self, ans: SbiRet) { let _ = ans; }
//! # }
//! let mut hart = VmHart::new();
//! loop {
//!     let trap = hart.run();
//!     if let Trap::Exception(Exception::SupervisorEcall) = trap.cause() {
//!         // Firstly, handle custom extensions
//!         let my_extension_sbiret = hart.my_extension_sbi.handle_ecall(hart.trap_params());
//!         // If the custom extension handles correctly, fill in its result and continue to hart.
//!         // The custom handler may handle `probe_extension` in `base` extension as well
//!         // to allow detections to whether a custom extension exists.
//!         if my_extension_sbiret != SbiRet::not_supported() {
//!             hart.fill_in(my_extension_sbiret);
//!             continue;
//!         }
//!         // Then, if it's not a custom extension, handle it using standard SBI handler.
//!         let standard_sbiret = hart.sbi.handle_ecall(hart.trap_params());
//!         hart.fill_in(standard_sbiret);
//!     }
//! }
//! ```
//!
//! RustSBI would interact well with custom extension environments in this way.
//!
//! ## Emulators using RustSBI
//!
//! RustSBI library may be used to write RISC-V emulators. Other than hardware accelerated binary
//! translation methods, emulators typically do not use host hardware-specific features,
//! thus may build and run on any architecture.
//! Like hardware RISC-V implementations, software emulated RISC-V environment would still need SBI
//! implementation to support supervisor environment.
//!
//! Writing emulators would follow the similar process with writing hypervisors, see
//! [Hypervisors using RustSBI](#hypervisors-using-rustsbi) for details.
//!
//! # Download binary file: the Prototyper vs. discrete packages
//!
//! RustSBI ecosystem would typically provide support for most platforms. Those support packages
//! would be provided either from the RustSBI Prototyper or vendor provided discrete SBI
//! implementation packages.
//!
//! The RustSBI Prototyper is a universal support package provided by RustSBI ecosystem.
//! It is designed to save development time while providing most SBI features possible.
//! It also includes a simple test kernel to allow testing SBI implementations on current environment.
//! Users may choose to download from [Prototyper repository](https://github.com/rustsbi/prototyper)
//! to get various types of RustSBI packages for their boards.
//! Vendors and contributors may find it easy to adapt new SoCs and boards into the Prototyper.
//!
//! Discrete SBI packages are SBI environment support packages specially designed for one board
//! or SoC, it will be provided by board vendor or RustSBI ecosystem.
//! Vendors may find it easy to include fine-grained features in each support package, but the
//! maintenance situation would vary between vendors, and it would likely cost a lot of time
//! to develop from a bare-metal executable. Users may find a boost in performance, energy saving
//! indexes and feature granularity in discrete packages, but it would depend on whether the
//! vendor provides it.
//!
//! To download binary package for the Prototyper, visit its project website for a download link.
//! To download them for discrete packages, RustSBI users may visit distribution sources of SoC or board
//! manufacturers. Additionally, users may visit [the awesome page](https://github.com/rustsbi/awesome-rustsbi)
//! for a curated list of both Prototyper and discrete packages provided by RustSBI ecosystem.
//!
//! # Notes for RustSBI developers
//!
//! Following useful hints are for firmware and kernel developers when working with SBI and RustSBI.
//!
//! ## RustSBI is a library for interfaces
//!
//! This library adapts to individual Rust traits and a derive-macro to provide basic SBI features.
//! When building for a specific platform, implement traits in this library and pass the types into
//! a structure to derive RustSBI macro onto. After that, [`handle_ecall`](trait.RustSBI.html#tymethod.handle_ecall)
//! would be called in the platform-specific exception handler.
//! The [derive macro `RustSBI`](derive.RustSBI.html) would dispatch parameters from supervisor
//! to the trait implementations to handle the SBI calls.
//!
//! The library also implements useful constants which may help with platform-specific binaries.
//! The `LOGO` and information on `VERSION` can be printed if necessary on SBI initialization
//! processes.
//!
//! Note that this crate is a library that contains common building blocks in SBI implementation.
//! The RustSBI ecosystem would provide different levels of support for each board, those support
//! packages would use `rustsbi` crate as a library to provide different types of SBI binary releases.
//!
//! ## Hardware discovery and feature detection
//!
//! According to the RISC-V SBI specification, the SBI itself does not specify any method for
//! hardware discovery. The supervisor software must rely on the other industry standard hardware
//! discovery methods (i.e., Device Tree, ACPI, vendor-specific ones or upcoming `configptr` CSRs)
//! for that purpose.
//!
//! To detect any feature under bare metal or under supervisor level, developers may depend on
//! any hardware discovery methods, or use try-execute-trap method to detect any instructions or
//! CSRs. If SBI is implemented in user level emulators, it may require to depend on operating
//! system calls or use a signal-trap procedure to detect any RISC-V core features.

#![no_std]

mod console;
mod cppc;
mod hsm;
mod ipi;
mod nacl;
mod pmu;
mod reset;
mod rfence;
mod sta;
mod susp;
mod timer;

mod forward;
mod traits;

/// The RustSBI logo without blank lines on the beginning.
pub const LOGO: &str = r".______       __    __      _______.___________.  _______..______   __
|   _  \     |  |  |  |    /       |           | /       ||   _  \ |  |
|  |_)  |    |  |  |  |   |   (----`---|  |----`|   (----`|  |_)  ||  |
|      /     |  |  |  |    \   \       |  |      \   \    |   _  < |  |
|  |\  \----.|  `--'  |.----)   |      |  |  .----)   |   |  |_)  ||  |
| _| `._____| \______/ |_______/       |__|  |_______/    |______/ |__|";

// RustSBI supports RISC-V SBI specification 2.0 ratified.
const SBI_SPEC_MAJOR: usize = 2;
const SBI_SPEC_MINOR: usize = 0;

/// RustSBI implementation ID: 4.
///
/// Ref: <https://github.com/riscv-non-isa/riscv-sbi-doc/pull/61>
const IMPL_ID_RUSTSBI: usize = 4;

const RUSTSBI_VERSION_MAJOR: usize = (env!("CARGO_PKG_VERSION_MAJOR").as_bytes()[0] - b'0') as _;
const RUSTSBI_VERSION_MINOR: usize = (env!("CARGO_PKG_VERSION_MINOR").as_bytes()[0] - b'0') as _;
const RUSTSBI_VERSION_PATCH: usize = (env!("CARGO_PKG_VERSION_PATCH").as_bytes()[0] - b'0') as _;
const RUSTSBI_VERSION: usize =
    (RUSTSBI_VERSION_MAJOR << 16) + (RUSTSBI_VERSION_MINOR << 8) + RUSTSBI_VERSION_PATCH;

/// RustSBI version as a string.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub extern crate sbi_spec as spec;

pub use sbi_spec::binary::{CounterMask, HartMask, Physical, SbiRet, SharedPtr};

/// Generate `RustSBI` implementation for structure of each extension.
///
/// # Usage
///
/// The `#[derive(RustSBI)]` macro provides a convenient way of building `RustSBI` trait implementations.
/// To use this macro, say that we have a struct `MyFence` with RISC-V SBI Remote Fence extension
/// implemented using `rustsbi::Fence` trait. Then, we build a struct around it, representing a
/// whole SBI implementation including one `Fence` extension only; we can name it `MySBI`:
///
/// ```rust
/// struct MySBI {
///     fence: MyFence,
/// }
///
/// struct MyFence { /* fields */ }
///
/// #
/// # use sbi_spec::binary::{SbiRet, HartMask};
/// impl rustsbi::Fence for MyFence {
///     /* implementation details */
/// #   fn remote_fence_i(&self, _: HartMask) -> SbiRet { unimplemented!() }
/// #   fn remote_sfence_vma(&self, _: HartMask, _: usize, _: usize) -> SbiRet { unimplemented!() }
/// #   fn remote_sfence_vma_asid(&self, _: HartMask, _: usize, _: usize, _: usize) -> SbiRet { unimplemented!() }
/// }
/// ```
///
/// Here, we declared the field named `fence` with type `MyFence`. The variable name `fence` is special,
/// it tells the RustSBI derive-macro that this field implements SBI Remote Fence instead of other SBI extensions.
/// We can continue to add more fields into `MySBI`. For example, if we have RISC-V SBI Time extension
/// implementation with type `MyTimer`, we can add it to `MySBI`:
///
/// ```rust
/// struct MySBI {
///     fence: MyFence,
///     timer: MyTimer,
/// }
/// # struct MyFence;
/// # struct MyTimer;
/// ```
///
/// Don't forget that the name `timer` is also a special field name. There is a detailed list after this
/// chapter describing what special field name would the `RustSBI` macro identify.
///
/// It looks like we are ready to derive `RustSBI` macro on `MySBI`! Let's try it now ...
///
/// ```compile_fail
/// #[derive(RustSBI)]
/// struct MySBI {
///     fence: MyFence,
///     timer: MyTimer,
/// #   #[cfg(feature = "machine")] info: () // compile would success on #[cfg(feature = "machine")], cause it always to fail
/// }
///
/// # use sbi_spec::binary::{SbiRet, HartMask};
/// # struct MyFence;
/// # impl rustsbi::Fence for MyFence {
/// #     fn remote_fence_i(&self, _: HartMask) -> SbiRet { unimplemented!() }
/// #     fn remote_sfence_vma(&self, _: HartMask, _: usize, _: usize) -> SbiRet { unimplemented!() }
/// #     fn remote_sfence_vma_asid(&self, _: HartMask, _: usize, _: usize, _: usize) -> SbiRet { unimplemented!() }
/// # }
/// # struct MyTimer;
/// # impl rustsbi::Timer for MyTimer {
/// #     fn set_timer(&self, stime_value: u64) { unimplemented!() }
/// # }
/// ```
///
/// Oops! Compile failed. We'd check what happened here:
///
/// ```text
/// error: can't derive RustSBI: #[cfg(feature = "machine")] is needed to derive RustSBI with no extra `EnvInfo` provided; consider adding an `info` parameter to provide machine information implementing `rustsbi::EnvInfo` if RustSBI is not run on machine mode.
///  --> example.rs:LL:10
///    |
/// LL | #[derive(RustSBI)]
///    |          ^^^^^^^
///    |
///  = note: this error originates in the derive macro `RustSBI` (in Nightly builds, run with -Z macro-backtrace for more info)
///
/// error: aborting due to previous error
/// ```
///
/// The error message hints that we didn't provide any SBI environment information implementing trait
/// `EnvInfo`. By default, RustSBI is targeted to provide RISC-V supervisor environment on any hardware,
/// focusing on hypervisor, emulator and binary translation applications. In this case, the virtualized
/// environment should provide the supervisor with machine environment information like `mvendorid`,
/// `marchid` and `mimpid` values. RustSBI could also be used on bare-metal RISC-V machines where such
/// values would be directly accessible through CSR read operations.
///
/// If we are targeting bare-metal, we can use the RustSBI library with `#[cfg(feature = "machine")]`
/// enabled by changing `dependencies` section in `Cargo.toml` file (if we are using Cargo):
///
/// ```toml
/// [dependencies]
/// rustsbi = { version = "0.4.0", features = ["machine"] }
/// ```
///
/// If that's not the case, and we are writing a virtualization-targeted application, we should add a
/// `EnvInfo` implementation into the structure like `MySBI` mentioned above, with the special field
/// name `info`. We can do it like:
///
/// ```rust
/// # use rustsbi::RustSBI;
/// #[derive(RustSBI)]
/// struct MySBI {
///     fence: MyFence,
///     timer: MyTimer,
/// #   #[cfg(not(feature = "machine"))]
///     info: MyEnvInfo,
/// }
///
/// struct MyEnvInfo;
///
/// impl rustsbi::EnvInfo for MyEnvInfo {
///     #[inline]
///     fn mvendorid(&self) -> usize { todo!("add real value here") }
///     #[inline]
///     fn marchid(&self) -> usize { todo!("add real value here") }
///     #[inline]
///     fn mimpid(&self) -> usize { todo!("add real value here") }
/// }
/// # use sbi_spec::binary::{SbiRet, HartMask};
/// # struct MyFence;
/// # impl rustsbi::Fence for MyFence {
/// #     fn remote_fence_i(&self, _: HartMask) -> SbiRet { unimplemented!() }
/// #     fn remote_sfence_vma(&self, _: HartMask, _: usize, _: usize) -> SbiRet { unimplemented!() }
/// #     fn remote_sfence_vma_asid(&self, _: HartMask, _: usize, _: usize, _: usize) -> SbiRet { unimplemented!() }
/// # }
/// # struct MyTimer;
/// # impl rustsbi::Timer for MyTimer {
/// #     fn set_timer(&self, stime_value: u64) { unimplemented!() }
/// # }
/// ```
///
/// Then, when we compile our code with `MySBI`, we'll find that the code now compiles successfully.
///
/// To use the derived `RustSBI` implementation, we note out that this structure now implements the trait
/// `RustSBI` with function `handle_ecall`. We can pass the SBI extension, function and parameters into
/// `handle_ecall`, and read the SBI call result from its return value with the type `SbiRet`.
/// To illustrate this feature, we make an SBI call to read the SBI implementation ID, like:
///
/// ```rust
/// # use rustsbi::RustSBI;
/// #[derive(RustSBI)]
/// struct MySBI {
///     /* we omit the extension fields by now */
/// #   info: MyEnvInfo,
/// }
///
/// fn main() {
///     // Create a MySBI instance.
///     let sbi = MySBI {
///         /* include initial values for fields */
/// #       info: MyEnvInfo
///     };
///     // Make the call. Read SBI implementation ID resides in extension Base (0x10),
///     // with function id 1, and it doesn't have any parameters.
///     let ret = sbi.handle_ecall(0x10, 0x1, [0; 6]);
///     // Let's check the result...
///     println!("SBI implementation ID for MySBI: {}", ret.value);
///     assert_eq!(ret.value, 4);
/// }
/// # struct MyEnvInfo;
/// # impl rustsbi::EnvInfo for MyEnvInfo {
/// #     fn mvendorid(&self) -> usize { unimplemented!() }
/// #     fn marchid(&self) -> usize { unimplemented!() }
/// #     fn mimpid(&self) -> usize { unimplemented!() }
/// # }
/// ```
///
/// Run the code, and we'll find the following output in the console:
///
/// ```text
/// SBI implementation ID for MySBI: 4
/// ```
///
/// The SBI call returns the number 4 as the SBI call result. By looking up
/// [the RISC-V SBI Specification](https://github.com/riscv-non-isa/riscv-sbi-doc/blob/cf86bda6f57afb8e0e7011b61504d4e8664b9b1d/src/ext-base.adoc#sbi-implementation-ids),
/// we can know that RustSBI have the implementation ID of 4. You have successfully made your first
/// SBI call from a derived `RustSBI` implementation!
///
/// If we learn further from the RISC-V privileged software architecture, we may know more about how
/// RISC-V SBI works on an environment to support supervisor software. RISC-V SBI implementations
/// accept SBI calls by supervisor-level environment call caused by `ecall` instruction under supervisor
/// mode. Each `ecall` raises a RISC-V exception which the environment must process with. The SBI
/// environment, either bare-metal or virtually, would save context, read extension, function and parameters
/// and call the `handle_ecall` function provided by `RustSBI` trait. Then, the function returns
/// with an `SbiRet`; we read back `value` and `error` to store them into the saved context.
/// Finally, when the context restores, the supervisor mode software (kernels, etc.) could get the
/// SBI call result from register values.
///
/// Additionally, the macro also provides a dynamic way of building `RustSBI` trait implementations.
/// It allows developers to dynamically choose which device to use in the actual RustSBI implementation.
/// To use this feature, consider two structs namely `FenceOne` and `FenceTwo`, both implementing
/// RISC-V SBI Remote Fence extension. We can now derive `RustSBI` macro on `MySBI` wrapping those
/// two structs with `Option` type, annotated with `#[rustsbi(dynamic)]`:
/// ``
///
/// ```rust
/// # use rustsbi::RustSBI;
/// #[derive(RustSBI)]
/// #[rustsbi(dynamic)]
/// struct MySBI {
///     fence: Option<FenceOne>,
///     rfnc: Option<FenceTwo>,
/// #   info: MyEnvInfo,
/// }
///
/// struct FenceOne { /* fields */ }
///
/// struct FenceTwo { /* fields */ }
///
/// // Both `FenceOne` and `FenceTwo` implements `rustsbi::Fence`.
///
/// # use sbi_spec::binary::{SbiRet, HartMask};
/// # impl rustsbi::Fence for FenceOne {
/// #     fn remote_fence_i(&self, _: HartMask) -> SbiRet { unimplemented!() }
/// #     fn remote_sfence_vma(&self, _: HartMask, _: usize, _: usize) -> SbiRet { unimplemented!() }
/// #     fn remote_sfence_vma_asid(&self, _: HartMask, _: usize, _: usize, _: usize) -> SbiRet { unimplemented!() }
/// # }
/// # impl rustsbi::Fence for FenceTwo {
/// #     fn remote_fence_i(&self, _: HartMask) -> SbiRet { unimplemented!() }
/// #     fn remote_sfence_vma(&self, _: HartMask, _: usize, _: usize) -> SbiRet { unimplemented!() }
/// #     fn remote_sfence_vma_asid(&self, _: HartMask, _: usize, _: usize, _: usize) -> SbiRet { unimplemented!() }
/// # }
/// # struct MyEnvInfo;
/// # impl rustsbi::EnvInfo for MyEnvInfo {
/// #     fn mvendorid(&self) -> usize { unimplemented!() }
/// #     fn marchid(&self) -> usize { unimplemented!() }
/// #     fn mimpid(&self) -> usize { unimplemented!() }
/// # }
/// ```
///
/// We have declared two fields, `fence` and `rfnc`, both of which are recognized by RustSBI as potential
/// candidates for the SBI Remote Fence extension. Dynamic RustSBI will probe both fields to determine
/// which field it should use for the SBI calls on the Remote Fence extension.
///
/// Now we have learned basic usages of the derive-macro `RustSBI`. We can dive deeper and use RustSBI
/// in real cases with ease. Congratulations!
///
/// # Supported extensions
///
/// The derive macro `RustSBI` supports all the standard RISC-V SBI extensions this library supports.
/// When we add extensions into SBI structure fields, special field names are identified by RustSBI
/// derive macro. Here is a list of them:
///
/// | Field names | RustSBI trait | Extension |
/// |:------------|:----------|:--------------|
/// | `time` or `timer` | [`Timer`](trait.Timer.html) | Timer programmer extension |
/// | `ipi` or `spi` | [`Ipi`](trait.Ipi.html) | S-mode Inter Processor Interrupt |
/// | `fence` or `rfnc` | [`Fence`](trait.Fence.html) | Remote Fence extension |
/// | `hsm` | [`Hsm`](trait.Hsm.html) | Hart State Monitor extension |
/// | `reset` or `srst` | [`Reset`](trait.Reset.html) | System Reset extension |
/// | `pmu` | [`Pmu`](trait.Pmu.html) | Performance Monitor Unit extension |
/// | `console` or `dbcn` | [`Console`](trait.Console.html) | Debug Console extension |
/// | `susp` | [`Susp`](trait.Susp.html) | System Suspend extension |
/// | `cppc` | [`Cppc`](trait.Cppc.html) | SBI CPPC extension |
/// | `nacl` | [`Nacl`](trait.Nacl.html) | Nested Acceleration extension |
/// | `sta` | [`Sta`](trait.Sta.html) | Steal Time Accounting extension |
///
/// The `EnvInfo` parameter is used by RISC-V SBI Base extension which is always supported on all
/// RISC-V SBI implementations. RustSBI provides the Base extension with additional `EnvInfo` by default.
///
/// | Field names | RustSBI trait | Description |
/// |:------------|:----------|:--------------|
/// | `info` or `env_info` | [`EnvInfo`](trait.EnvInfo.html) | Machine environment information used by Base extension |
///
/// Or, if `#[cfg(feature = "machine")]` is enabled, RustSBI derive macro does not require additional
/// machine environment information but reads them by RISC-V CSR operation when we don't have any `EnvInfo`s
/// in the structure. This feature would only work if RustSBI runs directly on machine mode hardware.
/// If we are targeting other environments (virtualization etc.), we should provide `EnvInfo` instead
/// of using the machine feature.
///
/// # Examples
///
/// This macro should be used over a struct of RISC-V SBI extension implementations.
/// For example:
///
/// ```rust
/// # use rustsbi::RustSBI;
/// #[derive(RustSBI)]
/// struct MySBI {
///     fence: MyFence,
///     info: MyEnvInfo,
/// }
///
/// // Here, we assume that `MyFence` implements `rustsbi::Fence`
/// // and `MyEnvInfo` implements `rustsbi::EnvInfo`.
/// # use sbi_spec::binary::{SbiRet, HartMask};
/// # struct MyFence;
/// # impl rustsbi::Fence for MyFence {
/// #     fn remote_fence_i(&self, _: HartMask) -> SbiRet { unimplemented!() }
/// #     fn remote_sfence_vma(&self, _: HartMask, _: usize, _: usize) -> SbiRet { unimplemented!() }
/// #     fn remote_sfence_vma_asid(&self, _: HartMask, _: usize, _: usize, _: usize) -> SbiRet { unimplemented!() }
/// # }
/// # struct MyEnvInfo;
/// # impl rustsbi::EnvInfo for MyEnvInfo {
/// #     fn mvendorid(&self) -> usize { 1 }
/// #     fn marchid(&self) -> usize { 2 }
/// #     fn mimpid(&self) -> usize { 3 }
/// # }
/// ```
///
/// Fields indicating the same extension (SBI extension or `EnvInfo`) shouldn't be included
/// more than once.
///
/// ```compile_fail
/// #[derive(RustSBI)]
/// struct MySBI {
///     fence: MyFence,
///     rfnc: MyFence, // <-- Second field providing `rustsbi::Fence` implementation
///     info: MyEnvInfo,
/// }
///
/// # use sbi_spec::binary::{SbiRet, HartMask};
/// # struct MyFence;
/// # impl rustsbi::Fence for MyFence {
/// #     fn remote_fence_i(&self, _: HartMask) -> SbiRet { unimplemented!() }
/// #     fn remote_sfence_vma(&self, _: HartMask, _: usize, _: usize) -> SbiRet { unimplemented!() }
/// #     fn remote_sfence_vma_asid(&self, _: HartMask, _: usize, _: usize, _: usize) -> SbiRet { unimplemented!() }
/// # }
/// # struct MyEnvInfo;
/// # impl rustsbi::EnvInfo for MyEnvInfo {
/// #     fn mvendorid(&self) -> usize { 1 }
/// #     fn marchid(&self) -> usize { 2 }
/// #     fn mimpid(&self) -> usize { 3 }
/// # }
/// ```
///
/// When using the `#[rustsbi(dynamic)]` attribute, it is possible to include multiple
/// fields that indicate the same SBI extension.
///
/// ```rust
/// # use rustsbi::RustSBI;
/// #[derive(RustSBI)]
/// #[rustsbi(dynamic)]
/// struct MySBI {
///     // Fields in `#[rustsbi(dynamic)]` structures are usually wrapped in `Option`s.
///     fence: Option<MyFence>,
///     rfnc: Option<MyFence>,
///     // ^ Both the two fields are identified as `rustsbi::Fence` implementation.
///     info: MyEnvInfo,
///     // ^ However, environment information should only be introduced *once*.
/// }
///
/// // RustSBI will sequentially examine these fields, starting from the first one.
/// // Upon encountering the first `Option` that is `Some`, it will utilize this
/// // field as the implementation.
/// // i.e., if the first field `fence` in `MySBI` is `Some`, RustSBI uses `fence`.
/// // Else, if the second field `rfnc` is `Some`, RustSBI uses `rfnc`.
/// // Otherwise, RustSBI returns `SbiRet::not_supported`.
///
/// # use sbi_spec::binary::{SbiRet, HartMask};
/// # struct MyFence;
/// # impl rustsbi::Fence for MyFence {
/// #     fn remote_fence_i(&self, _: HartMask) -> SbiRet { unimplemented!() }
/// #     fn remote_sfence_vma(&self, _: HartMask, _: usize, _: usize) -> SbiRet { unimplemented!() }
/// #     fn remote_sfence_vma_asid(&self, _: HartMask, _: usize, _: usize, _: usize) -> SbiRet { unimplemented!() }
/// # }
/// # struct MyEnvInfo;
/// # impl rustsbi::EnvInfo for MyEnvInfo {
/// #     fn mvendorid(&self) -> usize { 1 }
/// #     fn marchid(&self) -> usize { 2 }
/// #     fn mimpid(&self) -> usize { 3 }
/// # }
/// ```
///
/// The struct as derive input may include generics, specifically type generics, lifetimes,
/// constant generics and where clauses.
///
/// ```rust
/// # use rustsbi::RustSBI;
/// #[derive(RustSBI)]
/// struct MySBI<'a, T: rustsbi::Fence, U, const N: usize>
/// where
///     U: rustsbi::Timer,
/// {
///     fence: T,
///     timer: U,
///     info: &'a MyEnvInfo,
///     _dummy: [u8; N],
/// }
///
/// # use sbi_spec::binary::{SbiRet, HartMask};
/// # struct MyFence;
/// # impl rustsbi::Fence for MyFence {
/// #     fn remote_fence_i(&self, _: HartMask) -> SbiRet { unimplemented!() }
/// #     fn remote_sfence_vma(&self, _: HartMask, _: usize, _: usize) -> SbiRet { unimplemented!() }
/// #     fn remote_sfence_vma_asid(&self, _: HartMask, _: usize, _: usize, _: usize) -> SbiRet { unimplemented!() }
/// # }
/// # struct MyEnvInfo;
/// # impl rustsbi::EnvInfo for MyEnvInfo {
/// #     fn mvendorid(&self) -> usize { 1 }
/// #     fn marchid(&self) -> usize { 2 }
/// #     fn mimpid(&self) -> usize { 3 }
/// # }
/// ```
///
/// Inner attribute `#[rustsbi(skip)]` informs the macro to skip a certain field when
/// generating a RustSBI implementation.
///
/// ```rust
/// #[derive(RustSBI)]
/// struct MySBI {
///     console: MyConsole,
///     #[rustsbi(skip)]
///     fence: MyFence,
///     info: MyEnvInfo,
/// }
///
/// // The derived `MySBI` implementation ignores the `fence: MyFence`. It can now
/// // be used as a conventional struct field.
/// // Notably, a `#[warn(unused)]` would be raised if `fence` is not further used
/// // by following code; `console` and `info` fields are not warned because they are
/// // internally used by the trait implementation derived in the RustSBI macro.
/// # use rustsbi::RustSBI;
/// # use sbi_spec::binary::{SbiRet, Physical, HartMask};
/// # struct MyConsole;
/// # impl rustsbi::Console for MyConsole {
/// #     fn write(&self, _: Physical<&[u8]>) -> SbiRet { unimplemented!() }
/// #     fn read(&self, _: Physical<&mut [u8]>) -> SbiRet { unimplemented!() }
/// #     fn write_byte(&self, _: u8) -> SbiRet { unimplemented!() }
/// # }
/// # struct MyFence;
/// # impl rustsbi::Fence for MyFence {
/// #     fn remote_fence_i(&self, _: HartMask) -> SbiRet { unimplemented!() }
/// #     fn remote_sfence_vma(&self, _: HartMask, _: usize, _: usize) -> SbiRet { unimplemented!() }
/// #     fn remote_sfence_vma_asid(&self, _: HartMask, _: usize, _: usize, _: usize) -> SbiRet { unimplemented!() }
/// # }
/// # struct MyEnvInfo;
/// # impl rustsbi::EnvInfo for MyEnvInfo {
/// #     fn mvendorid(&self) -> usize { 1 }
/// #     fn marchid(&self) -> usize { 2 }
/// #     fn mimpid(&self) -> usize { 3 }
/// # }
/// ```
///
/// In some cases, we may manually assign fields to a certain SBI extension other than defaulting
/// to special names defined above, and sometimes we need to provide multiple SBI extensions
/// with one field only. By listing the extension names separated by comma in the helper attribute,
/// we can assign one or multiple SBI extensions to a field to solve the issues above.
///
/// ```rust
/// #[derive(RustSBI)]
/// struct MySBI {
///     console: MyConsole,
///     #[rustsbi(fence)]
///     some_other_name: MyFence,
///     info: MyEnvInfo,
/// }
///
/// // RustSBI will now use the `some_other_name` field implementing `rustsbi::Fence`
/// // as the implementation of SBI Remote Fence extension.
/// # use rustsbi::RustSBI;
/// # use sbi_spec::binary::{SbiRet, Physical, HartMask};
/// # struct MyConsole;
/// # impl rustsbi::Console for MyConsole {
/// #     fn write(&self, _: Physical<&[u8]>) -> SbiRet { unimplemented!() }
/// #     fn read(&self, _: Physical<&mut [u8]>) -> SbiRet { unimplemented!() }
/// #     fn write_byte(&self, _: u8) -> SbiRet { unimplemented!() }
/// # }
/// # struct MyFence;
/// # impl rustsbi::Fence for MyFence {
/// #     fn remote_fence_i(&self, _: HartMask) -> SbiRet { unimplemented!() }
/// #     fn remote_sfence_vma(&self, _: HartMask, _: usize, _: usize) -> SbiRet { unimplemented!() }
/// #     fn remote_sfence_vma_asid(&self, _: HartMask, _: usize, _: usize, _: usize) -> SbiRet { unimplemented!() }
/// # }
/// # struct MyEnvInfo;
/// # impl rustsbi::EnvInfo for MyEnvInfo {
/// #     fn mvendorid(&self) -> usize { 1 }
/// #     fn marchid(&self) -> usize { 2 }
/// #     fn mimpid(&self) -> usize { 3 }
/// # }
/// ```
/// ```rust
/// #[derive(RustSBI)]
/// struct MySBI {
///     #[rustsbi(ipi, timer)]
///     clint: Clint, // <-- RISC-V CLINT will provide both Ipi and Timer extensions.
///     info: MyEnvInfo,
/// }
///
/// // Both Ipi and Timer extension implementations are now provided by the
/// // `clint` field implementing both `rustsbi::Ipi` and `rustsbi::Timer`.
/// # use rustsbi::RustSBI;
/// # use sbi_spec::binary::{SbiRet, Physical, HartMask};
/// # struct Clint;
/// # impl rustsbi::Timer for Clint {
/// #     fn set_timer(&self, stime_value: u64) { unimplemented!() }
/// # }
/// # impl rustsbi::Ipi for Clint {
/// #     fn send_ipi(&self, _: HartMask) -> SbiRet { unimplemented!() }
/// # }
/// # struct MyEnvInfo;
/// # impl rustsbi::EnvInfo for MyEnvInfo {
/// #     fn mvendorid(&self) -> usize { 1 }
/// #     fn marchid(&self) -> usize { 2 }
/// #     fn mimpid(&self) -> usize { 3 }
/// # }
/// ```
///
/// RustSBI implementations usually provide regular structs to the derive-macro.
/// Alternatively, the RustSBI derive macro also accepts tuple structs or unit structs.
///
/// ```rust
/// # use rustsbi::RustSBI;
/// // Tuple structs.
/// // No field names are provided; the structure must provide helper attributes
/// // to identify the extensions for the RustSBI derive macro.
/// #[derive(RustSBI)]
/// struct MySBI(#[rustsbi(fence)] MyFence, #[rustsbi(info)] MyEnvInfo);
///
/// # use sbi_spec::binary::{SbiRet, HartMask};
/// # struct MyFence;
/// # impl rustsbi::Fence for MyFence {
/// #     fn remote_fence_i(&self, _: HartMask) -> SbiRet { unimplemented!() }
/// #     fn remote_sfence_vma(&self, _: HartMask, _: usize, _: usize) -> SbiRet { unimplemented!() }
/// #     fn remote_sfence_vma_asid(&self, _: HartMask, _: usize, _: usize, _: usize) -> SbiRet { unimplemented!() }
/// # }
/// # struct MyEnvInfo;
/// # impl rustsbi::EnvInfo for MyEnvInfo {
/// #     fn mvendorid(&self) -> usize { 1 }
/// #     fn marchid(&self) -> usize { 2 }
/// #     fn mimpid(&self) -> usize { 3 }
/// # }
/// ```
/// ```rust
/// // Unit structs.
/// // Note that `info` is required in non-machine environment; thus this crate
/// // only allows unit structs with `machine` feature. No extensions except Base
/// // extension are provided on unit struct implementations.
/// # use rustsbi::RustSBI;
/// # #[cfg(feature = "machine")]
/// #[derive(RustSBI)]
/// struct MySBI;
/// ```
///
/// # Notes
// note: the following documents are inherited from `RustSBI` in the `rustsbi_macros` package.
#[doc(inline)]
pub use rustsbi_macros::RustSBI;

pub use console::Console;
pub use cppc::Cppc;
pub use hsm::Hsm;
pub use ipi::Ipi;
pub use nacl::Nacl;
pub use pmu::Pmu;
pub use reset::Reset;
pub use rfence::Rfence as Fence;
pub use sta::Sta;
pub use susp::Susp;
pub use timer::Timer;

pub use forward::Forward;
pub use traits::{EnvInfo, RustSBI};

// Macro internal functions and structures

#[cfg(feature = "machine")]
#[doc(hidden)]
pub use traits::_rustsbi_base_bare;
#[doc(hidden)]
pub use traits::{
    _ExtensionProbe, _StandardExtensionProbe, _rustsbi_base_env_info, _rustsbi_console,
    _rustsbi_cppc, _rustsbi_fence, _rustsbi_hsm, _rustsbi_ipi, _rustsbi_nacl, _rustsbi_pmu,
    _rustsbi_reset, _rustsbi_sta, _rustsbi_susp, _rustsbi_timer,
};
#[doc(hidden)]
pub use traits::{
    _rustsbi_console_probe, _rustsbi_cppc_probe, _rustsbi_fence_probe, _rustsbi_hsm_probe,
    _rustsbi_ipi_probe, _rustsbi_nacl_probe, _rustsbi_pmu_probe, _rustsbi_reset_probe,
    _rustsbi_sta_probe, _rustsbi_susp_probe, _rustsbi_timer_probe,
};
