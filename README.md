# RustSBI

RISC-V Supervisor Binary Interface ([SBI](https://github.com/riscv-non-isa/riscv-sbi-doc/)) library in Rust; runs on M-mode or HS mode.

[![crates.io](https://img.shields.io/crates/v/rustsbi.svg)](https://crates.io/crates/rustsbi)
[![Documentation](https://docs.rs/rustsbi/badge.svg)](https://docs.rs/rustsbi)
![License](https://img.shields.io/crates/l/rustsbi.svg)

## Binary downloads

Most users would get RustSBI binary download from the RustSBI Prototyping System. Check out the link
[here](https://github.com/rustsbi/standalone) to download binary package for your platform.

Boards, SoC vendors and research groups would provide dedicated RustSBI package for supported platforms.
There are packages exists on [awesome-rustsbi](https://github.com/rustsbi/awesome-rustsbi): it is a curated list of
awesome things related to RustSBI, which includes some implementation projects maintained by individuals or the community.

Users on commercial boards may visit implementation specific distribution links depending on the platforms they need,
or consult vendors if they provide discrete RustSBI package support.

## Minimum supported Rust version

To compile RustSBI library, you need at least stable Rust version of `rustc 1.65.0`.

If you are using feature `singleton` to support singleton based interface, you are required to install
nightly Rust compiler. You may need at least nightly Rust version of `rustc 1.59.0-nightly (c5ecc1570 2021-12-15)`.

## Build this project

RustSBI is usually used as a library or dependency. If you wish to, you may build RustSBI library itself using the
following command:

```bash
# If you don't have the cross compile target installed, install it first
rustup target add riscv64imac-unknown-none-elf
# Build this project as library
cargo build --target riscv64imac-unknown-none-elf
```

The build should finish without any errors.

If you see any error like `invalid register a0: unknown register`, it's likely that you cross built this project into
platforms other than RISC-V. RustSBI adapts to RISC-V SBI interface, so you may cross build this project into any bare
metal RISC-V platform targets instead.

The target platform of RustSBI is usually a bare metal target. Under normal circumstances these targets in Rust would
start with `riscv??-` and end with `-none-elf`. If a non-bare metal target is built to, it would throw build error
in `riscv` dependency crate or RustSBI library itself.

## Features

- Feature rich and extensible operating system runtime
- Empower support, compatibility for machines, hypervisors and emulators
- Support to and develop with RISC-V SBI specification v2.0-rc1
- Written in Rust, builds under stable Rust
- Capable to develop with other firmware ecosystem projects
- Adapted for operating system kernel models on your choice

## Frequently asked questions

1. How would I build a RustSBI implementation?

RustSBI have extensive documents on such purposes! No matter what you are building with it, you will find some
documents about RustSBI on bare-metal environments, hypervisors and emulators.

Check it out at [RustSBI document main page](https://docs.rs/rustsbi).

2. Can I use RustSBI on C based kernels?

Yes, you can! RustSBI strictly follows RISC-V SBI standard. All features are prepares for all programming languages,
as long as they support RISC-V SBI defined calling convention. 

If your kernel language supports other SBI implementations, usually it will support RustSBI in the same way.

## Talks and documents

This project is originally a part of rCore Summer of Code 2020 activities, inspired
by [MeowSBI](https://github.com/meow-chip/MeowSBI) and other similar projects. Now it is capable of running
rCore-Tutorial and other OS kernels on wide supported RISC-V devices.

There are multiple talks related to RustSBI dated back to Aug 2020. These talks discuss from design and abstraction of
RustSBI modules, to actual production and research usage scenario related to RustSBI and RISC-V bootloaders. Public
slides and blog articles of these talks are available at [RustSBI/slides](https://github.com/rustsbi/slides) page.

## Notes on platform implementation

1. RustSBI should be used as a library. Under normal circumstances, RustSBI platform can be implemented
   with embedded Rust's `embedded-hal` libraries.
2. Contributions are welcomed! We welcome to implement and test RustSBI for both FPGA cores and real cores.
   Implementations for emulators are also welcomed. If you are ready, start your own binary project and use
   RustSBI in it!
3. If there is a bug in RustSBI project itself, fire an issue or pull request to let us know! 

## License & Copyright

This project is licensed under either of

- MIT license ([LICENSE-MIT](LICENSE-MIT) or [http://opensource.org/licenses/MIT](http://opensource.org/licenses/MIT))
- Mulan PSL v2 ([LICENSE-MULAN](LICENSE-MULAN) or [https://opensource.org/licenses/MulanPSL-2.0](https://opensource.org/licenses/MulanPSL-2.0))

Documents from RISC-V SBI Specification are used in this project. These documents are (C) RISC-V Foundation under
Creative Commons Attribution 4.0 International License (CC-BY 4.0). The full license text is available
at https://creativecommons.org/licenses/by/4.0/.
