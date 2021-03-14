# RustSBI

RISC-V Supervisor Binary Interface ([SBI](https://github.com/riscv/riscv-sbi-doc/)) implementation in Rust; runs on M-mode.

[![crates.io](https://img.shields.io/crates/v/rustsbi.svg)](https://crates.io/crates/rustsbi)
[![Documentation](https://docs.rs/rustsbi/badge.svg)](https://docs.rs/rustsbi)
![License](https://img.shields.io/crates/l/rustsbi.svg)

## Binary downloads

See [releases](https://github.com/luojia65/rustsbi/releases).

Binaries are available for platforms which can be found on
[platform support page](https://github.com/luojia65/rustsbi/tree/master/platform).
This page includes an instruction to write your own RustSBI implementation on production use,
and some reference implementations for experiments.

## Features

- Functional and extensible operating system runtime
- Fully support to RISC-V SBI specification v0.2
- Adapted for unix-like operating systems
- Written in Rust
- Competitive to OpenSBI with most of its function
- Bundled with a test framework for SBI implementations
- Supports QEMU emulator (priv. spec v1.11)
- Backward compatible to Kendryte K210 with spec v1.9, MMU and S-Mode

## Components

The RustSBI project contains three parts: the RustSBI library `rustsbi`, reference implementation
in `platform`, and a simple operating system kernel to test SBI implementations in `test-kernel`.

The test kernel is used to test SBI functions. Boot this kernel using your platform,
it will call all SBI calls and run instructions to test if underlying SBI environment is okay.
if this kernel reports 'SUCCESS' and exits normally, it means that your SBI implementation is correct.

## Talks and documents

This project is originally a part of rCore Summer of Code 2020 activities, now it is
capable of running rCore-Tutorial and other OS kernels on wide supported RISC-V devices.

Blog article (Chinese):

- [Rust in Embedded World](https://www.yuque.com/chaosbot/rust_magazine_2021/biydon), Jan 2021
- [rCore Operating System Lab Final Report](https://github.com/luojia65/rcore-os-blog/blob/master/source/_posts/os-report-final-luojia65.md), Aug 2020

Slides (Chinese):

- [Design and Implementation of RustSBI](https://github.com/luojia65/DailySchedule/blob/master/2020-slides/RustSBI%E7%9A%84%E8%AE%BE%E8%AE%A1%E4%B8%8E%E5%AE%9E%E7%8E%B0.pdf), Dec 2020
- [The Rust Embedded System Development](https://github.com/luojia65/DailySchedule/blob/master/2020-slides/Rust%E5%B5%8C%E5%85%A5%E5%BC%8F%E5%BC%80%E5%8F%91.pdf), Dec 2020
- [Operating Systems on Rust and RISC-V](https://github.com/luojia65/DailySchedule/blob/master/2020-slides/Rust%E8%AF%AD%E8%A8%80%E4%B8%8ERISC-V%E6%93%8D%E4%BD%9C%E7%B3%BB%E7%BB%9F.pdf), Aug 2020

## Notes on platform implementation

1. RustSBI can be used as a library. Under normal circumstances, RustSBI platform can be implemented
   with embedded Rust's `embedded-hal` libraries.
2. On both QEMU and K210 platform, we supports CLINT and PLIC peripherals. Embedded Rust's community
   still need more SoCs taped out to discuss on common libraries on RISC-V ecosystem. After these works
   are done, we may use crates then to implement QEMU, without the `hal` module we have now.
3. Contributions are welcomed! We welcome to implement RustSBI for both FPGA cores and real cores.
   Implementations for emulators are also welcomed. Fire a pull request if you are ready!

## License & Copyright

This project is licensed under either of

- MIT license ([LICENSE-MIT](LICENSE-MIT) or [http://opensource.org/licenses/MIT](http://opensource.org/licenses/MIT))
- Mulan PSL v2 ([LICENSE-MULAN](LICENSE-MULAN) or [https://opensource.org/licenses/MulanPSL-2.0](https://opensource.org/licenses/MulanPSL-2.0))

This project contains documents from [RISC-V SBI specification](https://github.com/riscv/riscv-sbi-doc)
repository. These documents are (C) RISC-V community under CC-BY 4.0 license.
