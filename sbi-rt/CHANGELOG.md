# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/), and this project adheres
to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.0.3] - 2024-02-08

This version adds support to the RISC-V SBI Specification version 2.0 ratified.

### Added

- Debug Console extension support
- `pmu_counter_fw_read_hi` in PMU extension
- Support for SBI CPPC extension
- Support for NACL and STA extensions
- Re-export `HartMask`, `Physical`, `SbiRet` and `SharedPtr` on `sbi-rt` library root

### Modified

- SPI and RFNC extensions now use `HartMask` parameter

### Fixed

- Minor document fixes on `ConfigFlags` and `set_timer`
- Document fixes on the HSM extension from SBI 2.0
- Extension ID of the Nacl and Sta extensions.

## [0.0.2] - 2022-10-10

In this version, we changed API style to trait-based type parameters, which would make it easier to
check parameter types at runtime to reduce errors.
If user chooses to use `integer-impls` feature, it would fall back to older style functions using integer types.

### Added

- Trait-based type parameter for all extensions
- Feature `integer-impls` to allow fast prototyping with sbi-rt crate
- Feature `legacy` to gate the SBI legacy extension
- Documents on various functions

### Modified

- Update `sbi-spec` to version 0.0.4, re-export `Version` structure
- Function `probe_extension` now returns an `ExtensionInfo` value
- Function `pmu_num_counters` returns a `usize` value

[Unreleased]: https://github.com/rustsbi/sbi-rt/compare/v0.0.3...HEAD
[0.0.3]: https://github.com/rustsbi/sbi-rt/compare/v0.0.2...v0.0.3
[0.0.2]: https://github.com/rustsbi/sbi-rt/compare/v0.0.1...v0.0.2
[0.0.1]: https://github.com/rustsbi/sbi-rt/releases/tag/v0.0.1
