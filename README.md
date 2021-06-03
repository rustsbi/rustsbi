# RustSBI

RISC-V Supervisor Binary Interface ([SBI](https://github.com/riscv/riscv-sbi-doc/)) implementation in Rust; runs on M-mode.

[![crates.io](https://img.shields.io/crates/v/rustsbi.svg)](https://crates.io/crates/rustsbi)
[![Documentation](https://docs.rs/rustsbi/badge.svg)](https://docs.rs/rustsbi)
![License](https://img.shields.io/crates/l/rustsbi.svg)

## Binary downloads

From version 0.2.0, RustSBI is reformed into a library, thus no longer provides binary downloads for specific platforms. 
You may visit RustSBI-QEMU, RustSBI-K210 or other projects depending on the platforms you need. 

## Features

- Functional and extensible operating system runtime
- Fully support to RISC-V SBI specification v0.2
- Adapted for operating system kernel models on your choice
- Written in Rust
- Competitive to OpenSBI with most of its function
- Bundled with a test framework for SBI implementations
- Empower support, compatibility for different platforms

## Frequently asked questions

1. Can I use RustSBI on C based kernels?

Yes, you can! RustSBI strictly follows RISC-V SBI standard. All features are prepares for all programming languages,
as long as they support RISC-V SBI defined calling convention. 

If your kernel language supports other SBI implementations, typically it will support RustSBI in the same way.

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
2. Contributions are welcomed! We welcome to implement RustSBI for both FPGA cores and real cores.
   Implementations for emulators are also welcomed. Fire a pull request if you are ready!

## License & Copyright

This project is licensed under either of

- MIT license ([LICENSE-MIT](LICENSE-MIT) or [http://opensource.org/licenses/MIT](http://opensource.org/licenses/MIT))
- Mulan PSL v2 ([LICENSE-MULAN](LICENSE-MULAN) or [https://opensource.org/licenses/MulanPSL-2.0](https://opensource.org/licenses/MulanPSL-2.0))

Documents from RISC-V SBI Specification are used in this project. These documents are (C) RISC-V Founcation 
under Creative Commons Attribution 4.0 International License (CC-BY 4.0).
The full license text is available at https://creativecommons.org/licenses/by/4.0/.
