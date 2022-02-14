# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/), and this project adheres
to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## Unreleased

### Added

### Modified

## [0.2.1] - 2022-02-14

This update fixes a severe bug on IPI module. The previous version of RustSBI did not follow the SBI definition of IPI
module on SBI v0.3 format. Users are encouraged to use 0.2.1 and newer version instead of yanked 0.2.0 version.

### Modified
- Internal speed up to new SBI v0.3+ IPI procedure
- Reduce code size by inlining internal functions

### Fixed
- Severe bug on IPI does not follow new SBI version convention rule
- Pass cargo test on docs, add test cases on hart mask

## [0.2.0] - 2022-02-13

### Added
- Support for RISC-V SBI v0.3 Specification
- S-level Illegal instruction exception is now delegated into S-level software handler
- Support RFENCE extension in RustSBI framework
- Added a test kernel to test SBI function on RustSBI implementations
- Support device tree binary in K210 platform
- Support SBI v0.3 hart suspend function
- Support PMU extension trait and init function
- Use fat pointer cell to support asynchronous hart state monitor module
- Build under new asm! macro

### Modified
- Reform RustSBI project into a library
- Function `rustsbi::ecall` now require `a0`-`a5` input parameters
- Enhanced in-line code documents from SBI standard
- Now IPI module requires to return an `SbiRet` value
- Remove use of `global_asm` and `llvm_asm` in test kernel
- Align to 4 bytes for interrupt handler on QEMU and test kernel
- Update `riscv` crate dependency for QEMU platform
- Use `mtval` to read instruction on QEMU; still need to be kept on K210 as 1.9.1 does not define this register behavior
- Modify second parameter of `enter_privileged` to `opaque` other than `dtb_pa`
- Dump all trap frame registers when exception happened in reference implementations
- Use `embedded-hal` dependency version `0.2.6`
- Change to asynchronous lock structure trait style
- Function `num_counters` returns `usize` and its SBI call must return `SBI_SUCCESS`
- Use amo mutex for legacy stdio handler; remove dependency on `lazy_static` and `spin`
- Improve documents to adapt to v1.0-rc2 specification

### Fixed
- Test kernel console now will lock before `println` line is finished
- Non-legacy supervisor IPI extension is fixed
- Returns -1 other than 0 when legacy console getchar function fails; thank you @duskmoon314

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

[Unreleased]: https://github.com/rustsbi/rustsbi/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/rustsbi/rustsbi/compare/v0.1.1...v0.2.0
[0.1.1]: https://github.com/rustsbi/rustsbi/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/rustsbi/rustsbi/compare/v0.0.2...v0.1.0
[0.0.2]: https://github.com/rustsbi/rustsbi/releases/tag/v0.0.2
