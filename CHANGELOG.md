# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.0] - 2021-02-23
### Added
- S-level Illegal instruction exception is now delegated into S-level software handler
- Support RFENCE extension in RustSBI framework
- Added a test kernel to test SBI function on RustSBI implementations

### Modified
- Function `rustsbi::ecall` now require 5 input parameters
- Enhanced in-line code documents from SBI standard
- Now IPI module requires to return an `SbiRet` value
- Remove use of `global_asm` and `llvm_asm` in test kernel
- Align to 4 bytes for interrupt handler on QEMU and test kernel
- Update `riscv` crate dependency for QEMU platform
- Use `mtval` to read instruction on QEMU; still need to be kept on K210 as 1.9.1 does not define this register behavior

### Fixed
- Test kernel console now will lock before `println` line is finished
- Non-legacy supervisor IPI extension is fixed

## [0.1.1] - 2021-02-01
### Added
- Abstract support for HSM and SRST extensions
- Support SRST extension using test device on QEMU
- Count harts from device tree binary on QEMU platform
- Show hart id on panic for QEMU platform

### Modified
- Use '#[naked]' instead of global assembly in newer Rust version for RustSBI platforms

### Fixed
- Fix `init_hsm` function which is not exported before
- Small fixes on library documents

## [0.1.0] - 2020-12-26
RustSBI is adapted to SBI standard with implementation number 4.
### Added
- Implementation specific SBI module `0x0A000004` defined
- K210 specific sbi_rustsbi_k210_sext SBI call

### Modified
- Update private SBI function to K210 implementation

### Fixed
- Delegate instruction load/store faults to S mode, allowing legacy console getchar to work on K210 (#7).
- Fixed 64-bit and 32-bit instruction value for target pointer widths
- Fixed readme document path for crates.io

## [0.0.2] - 2020-10-20
### Added
- Support for Kendryte K210 with MMU and S-Mode
- Support for QEMU
- SBI v0.2 TIME extension and IPI extension
- RISC-V ISA both RV32 and RV64
- RISC-V Privileged Specification v1.11
- Backward compatible to privileged spec v1.9.1

[Unreleased]: https://github.com/luojia65/rustsbi/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/luojia65/rustsbi/compare/v0.1.1...v0.2.0
[0.1.1]: https://github.com/luojia65/rustsbi/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/luojia65/rustsbi/compare/v0.0.2...v0.1.0
[0.0.2]: https://github.com/luojia65/rustsbi/releases/tag/v0.0.2
