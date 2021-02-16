# RustSBI platforms

Currently provided reference platform support projects:

| Platform | Legacy | Base | IPI | Timer | RFENCE | HSM | SRST | Note |
|:---------|:------:|:----:|:---:|:-----:|:------:|:---:|:----:|:-----|
| [Kendryte K210](./k210) | √ | √ | √ | √ | P | P | P | Privileged spec version: 1.9.1 |
| [QEMU](./qemu)          | √ | √ | √ | √ | P | P | √ | - |

P: Pending

## Notes for binary releases

These platform implementations are only for reference.
Although binaries are released along with RustSBI library itself,
platform developers should consider using RustSBI as a library,
other than adding code into forked project's 'platforms' and make a pull request.

The RustSBI project will release these platform binaries in release page.
A RustSBI implementation in production is typically a separate project other than squashed into 'platform' path;
but if you really want to contribute to these reference implementations, you may need to build these
platform packages by yourself.

### Build and install

To build provided reference platforms, you should install the command runner `just`.
Like `make`, [`just`](https://github.com/casey/just#just) is a handy way to save and run project specific commands.
To install just, refer to `just` packages [link](https://github.com/casey/just#packages) and pick
a install command according to your operating system used for development.

Each reference platform have provided `justfile` configuration file.
You may `cd` into project path and run command `just build` to build binary package.
The binary package should be ready in some place under `target` folder.
Or, use `just run` to build and execute it in emulator with a test kernel.

## Support your own platform

There are RISC-V cores, SoC's, and boards, we can not support them one by one with any software,
typically platform vendor should provide their own SBI support package.
RustSBI is designed as a 'building block' library to help on these support packages.

To implement for their own platforms, vendor would typically follow these steps:

1. Pick an RISC-V software runtime
2. Initialize SBI functions into RustSBI
3. Add additional support features

### Pick a runtime

When the processor is powering up, it executes few instructions to jump to a processor specific entry address.
Your SBI implementation should be there, ready to be executed to prepare an environment for Rust code
and interrupt environment.

This runtime should contain two parts: programming language runtime and interrupt handler.
Typically, processor vendor or SoC vendor should provide with a minimal runtime.
Or else, [`riscv-rt`](https://github.com/rust-embedded/riscv-rt) project would be okay to be a runtime
for all RISC-V chips, this project is provided by community to minimally support Rust language on
standard RISC-V hardware.

If we have to write own runtime, the runtime should initialize `bss` and `data` sections.
Crate [`r0`](https://github.com/rust-embedded/r0) would help a lot if we do this work manually.

When you begin to work with custom hardware e.g. interrupt controller, you should use vendor
provided platform specific packages. Load trap configurations into registers in vendor defined procedures.
This will provide a machine level trap handler for SBI implementation.

After you pick a runtime, there should be one main entry function and a trap entry function.

### Initialize and use SBI functions

All SBI modules in RustSBI are described as Rust traits. Read RISC-V SBI manual carefully,
or research on the operating systems you want to support to make a list of modules your SBI should support.

Although it's a legacy function, `console_putchar` function by now is commonly used as an early debug output.
Use an on-board serial transmitter half, or other ways to implemenet this function's corresponding Rust trait.
Then, use `init_legacy_stdio` etc. to load this module into RustSBI. RustSBI's documentation lists all these
traits that RustSBI supports.

Basical function like `mvendorid` and module detections are handled by RustSBI.
After the modules are initialized, RustSBI automatically implement module detection functions
and provide an `ecall` handler. You should use this `ecall` handler in runtime defined trap handler,
see documentation of `rustsbi::ecall` for details.

RustSBI implmenetaion may print `rustsbi::LOGO` and `misa` onto early debug screen.
There are also some functions in trap handler and entry which a typical SBI implmentation would write.
Here's a checklist of these functions:

- Delegate exceptions and interrupts to S privilege in `medeleg` and `mideleg`;
- Emulate `rdtime` instruction in illegal instruction exception;
- Raise invalid instruction to supervisor if `mstatus::MPP` is not `Machine`.

When we write code for SBI functions, `embedded-hal` packages would be helpful.
Some of these packages provide a universal abstract for SoC peripherals and are normally provided by SoC vendor.
If our board uses peripheral outside the SoC, other packages would come into effect.

After SBI functions are written, you should use `enter_privileged` function provided by RustSBI.
This function uses an entry address of operating system where RustSBI would change privilege level
and jump into. It would also requires a second opaque parameter that the OS would make use of.
Now your operating system should boot up and function normally. Congratulations!

### Additional support features

Your prototype bootloader is functional now, to complete its design you may provide more features
as a software product.

On PC, SBI software would be flashed onto unique flash or small MCU to help boot the processor.
The processor may be upgraded or replaced. You should check if the processor and its memory is okay.
Pick or design a small program to check all RISC-V registers, instructions to check if the processor
is not broken and is ready to run operating system.

According to your platform, there could be a minimal hardware requirement. Personal computer platform
may require a graphic hardware to boot, you should scan if your processor or board provides one graphic
hardware. Scan other hardware to prepare for later boot sequences, like setting CPU clocks.

The operating system could be placed anywhere. According to your platform, you may scan for storage
or connect to the Internet for a functional operating system kernel. Fetch the kernel and provide
its entry address into `enter_privileged` as mentioned above.
If multiple operating systems are present, provide a user interface to help your user to select which
kernel we should boot.

During boot procedure, provide a way to debug hardware errors. For servers and developers,
activate your on-board buzzer, or provide a debug interface like JTAG to tell where the error comes from.
For DIY users and overclockers, you may provide EZDebug lights on your board and use SBI to control them.

A full boot sequence may be different on different vendors, and wrong settings on boot sequence can be
hard to debug. Read its manual carefully or consult the vendor to work out common bugs.

## Reference

Implement your SBI platform is easy! There exists some reference implementations in addition to this repository.

- `terminus_bl` project is a RustSBI implementation for `terminus` emulator: [shady831213/terminus_bl](https://github.com/shady831213/terminus_bl).
