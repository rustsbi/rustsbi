# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] - 2020-12-26
RustSBI is adapted to SBI standard with implementation number 4.
### Added
- K210 specific sbi_rustsbi_k210_sext SBI call

### Modified
- Update private SBI function to K210 implementation

### Fixed
- Delegate instruction load/store faults to S mode, allowing legacy console getchar to work on K210 (#7).
- Fixed 64-bit and 32-bit instruction value for target pointer widths

## [0.0.2] - 2020-10-20
### Added
- Support for Kendryte K210 with MMU and S-Mode
- Support for QEMU
- SBI v0.2 TIME extension and IPI extension
- RISC-V ISA both RV32 and RV64
- RISC-V Privileged Specification v1.11
- Backward compatible to privileged spec v1.9.1

[Unreleased]: https://github.com/olivierlacan/keep-a-changelog/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/olivierlacan/keep-a-changelog/compare/v0.0.1...v0.1.0
[0.0.2]: https://github.com/olivierlacan/keep-a-changelog/releases/tag/v0.0.2
