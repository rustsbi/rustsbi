# `sbi-spec`: RISC-V SBI constants and structures in Rust

[![CI](https://github.com/rustsbi/sbi-spec/actions/workflows/build.yml/badge.svg?branch=main)](https://github.com/rustsbi/sbi-spec/actions)
[![Latest version](https://img.shields.io/crates/v/sbi-spec.svg)](https://crates.io/crates/sbi-spec)
[![issue](https://img.shields.io/github/issues/rustsbi/sbi-spec)](https://github.com/rustsbi/sbi-spec/issues)
[![Documentation](https://docs.rs/sbi-spec/badge.svg)](https://docs.rs/sbi-spec)
![license](https://img.shields.io/github/license/rustsbi/sbi-spec)

This library implements the constants and structures defined in the [SBI Specification](https://github.com/riscv-non-isa/riscv-sbi-doc) using Rust.

## Supported RISC-V SBI extensions

Implementation status of the 3.0 specification by chapters:

| # | Title | Support state |
|:--|:------|:--------------|
| §3 | Binary Encoding | Constants, Structures | 
| §4 | Base Extension | Constants |
| §5 | Legacy Extensions | Constants |
| §6 | Timer Extension | Constants |
| §7 | IPI Extension | Constants |
| §8 | RFENCE Extension | Constants |
| §9 | Hart State Management Extension | Constants |
| §10 | System Reset Extension | Constants |
| §11 | Performance Monitoring Unit Extension | Constants |
| §12 | Debug Console Extension | Constants |
| §13 | System Suspend Extension | Constants |
| §14 | CPPC Extension | Constants |
| §15 | Nested Acceleration Extension | Constants |
| §16 | Steal-time Accounting Extension | Constants |
| §17 | Supervisor Software Events Extension | Constants |
| §18 | Firmware Features Extension | Constants |
| §19 | Debug Triggers Extension | Constants |
| §20 | Message Proxy Extension | Constants |

Although deprecated, legacy extensions are retained under `#[cfg(feature = "legacy")]` to ensure
compatibility with older software.

## License & Copyright

This project is licensed under either of

- MIT license ([LICENSE-MIT](LICENSE-MIT) or [http://opensource.org/licenses/MIT](http://opensource.org/licenses/MIT))
- Mulan PSL v2 ([LICENSE-MULAN](LICENSE-MULAN) or [https://opensource.org/licenses/MulanPSL-2.0](https://opensource.org/licenses/MulanPSL-2.0))

Documents from RISC-V SBI Specification are used in this project. These documents are (C) RISC-V Foundation under
Creative Commons Attribution 4.0 International License (CC-BY 4.0). The full license text is available
at https://creativecommons.org/licenses/by/4.0/.
