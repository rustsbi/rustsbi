# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- pmu: add config flags with bitflags in chapter 11
- fwft: add support for FWFT extension in chapter 18
- sse: add support for Supervisor Software Events Extension in chapter 17
- binary: add `HartIds` structure for an iterator over `HartMask`
- Support `DBTR` extension in Chapter 19
- mpxy: add support for MPXY extension in chapter 20
- binary: `impl From<Error> for SbiRet`, `impl IntoIterator for SbiRet`
- binary: unsafe functions `SbiRet::{unwrap_unchecked, unwrap_err_unchecked}`
- binary: internal unit tests for `SbiRet` constructors
- examples: simple RV128I emulator example
- examples: an SBI version example for usage of the Version structure
- base: add special constant `V1_0` and `V2_0` for structure `Version`
- examples: add an example on non-usize `HartMask` structure
- examples: add an example for custom SBI error code
- binary: add `TriggerMask` structure, it would be used in SBI DBTR extension
- binary: add `SbiRet::denied_locked()` error code

### Modified

- Migrate sbi-rt crate to Rust 2024 edition.
- base: don't derive `PartialOrd` for `Version`, instead manually implement `Ord` and forward it into `PartialOrd`.
- base: refactor `SbiRet` to be generic of registers and introduce the `SbiRegister` trait
- base: implement `SbiRegister` for `i32`, `i64`, `i128` and `isize` primitive types
- base: make HartMask and CounterMask generic over SBI registers
- Add C language naming alias tags to all constants in the sbi-spec package
- binary: refactor code to split binary structures into modules

### Fixed

- Remove redundant license file on module path; the `sbi-spec` module inherits workspace level license files.
- Fix typos.

## [0.0.8] - 2024-10-25

### Added

- base: add Coreboot and Oreboot to `impl_id` module
- binary: add counter index mask type ([#71](https://github.com/rustsbi/rustsbi/pull/71))
- pmu: add `shmem_size` module for PMU snapshot shared memory, add unit test for `pmu::shmem_size::SIZE`
- binary: add function `is_ok_and`, `is_err_and`, `inspect` and `inspect_err` for `SbiRet` structure
- base: impl `Eq`, `PartialEq`, `Ord`, `PartialOrd` and `Hash` for `Version`, add unit tests

### Modified

- binary: amend documentation on `SbiRet::denied()` error.
- binary: change `SbiRet::and` signature to `fn and<U>(self, res: Result<U, Error>) -> Result<U, Error>`

### Fixed

- pmu: fix serial number issues in docs.

## [0.0.7] - 2024-02-05

`sbi-spec` crate now supports RISC-V SBI version 2.0 ratified.

### Added

- Support to PMU events in Chapter 11
- Support `NACL` extension in Chapter 15
- Support `STA` extension in Chapter 16
- Add new SBI error `NoShmem`
- binary: add `SharedPtr` struct to represent shared memory range feature.
- nacl: add `shmem_size` module
- Move `HartMask` structure to `sbi-spec` crate from `rustsbi` crate.

### Modified

- Rearrange `HSM` constants into modules.

### Fixed

- Remove redundant prefixes in `PMU`
- Add new function id `SNAPSHOT_SET_SHMEM` in `PMU`
- Grammar fixes on documents

## [0.0.6] - 2023-04-04

### Added

- `Physical` shared memory physical address range with type annotation in Chapter 3
- Support to RISC-V SBI System Suspend extension
- Support to CPPC extension

## [0.0.5] - 2023-02-16

### Added

- Adapt to RISC-V SBI specification version 2.0-rc1
- `PMU_COUNTER_FW_READ_HI` function in `pmu` module for RV32 systems
- SBI DBCN extension support
- `Result`-like documents to `SbiRet`

### Modified

- style: add period to docs

## [0.0.4] - 2022-10-10

### Added

- Various convenient functions to `SbiRet` structure
- Add documents on whole `sbi-rt` crate to coply with `deny(missing_docs)`
- Feature `legacy` to gate legacy SBI extension

### Modified

- Rename `SbiRet::ok` to `SbiRet::success`
- Rename `SbiSpecVersion` to struct `Version`

## [0.0.3] - 2022-10-06

### Added

- deps: static_assertions
  check implementations during compilation, and provide an item list for developers
- denied: warnings and unsafe code
- a github workflow to check building
- `SbiSpecVersion` type definition for sbi base

### Modified

- rename `GET_SPEC_VERSION` to `GET_SBI_SPEC_VERSION`
- rename `impl_id::IMPL_XXX` to `impl_id::XXX`

### Removed

- default target to RISC-V

## [0.0.2] - 2022-07-21

### Added

- A changelog to this project

### Modified

- Lift build target limit; now this crate would build on targets other than RISC-V

## [0.0.1] - 2022-07-11

This is the first release of sbi-spec crate. This crate includes definition of RISC-V Supervisor Binary Interface (SBI) including structures and constants.

### Added

- Adapt to SBI specification version 1.0.0 ratified

[Unreleased]: https://github.com/rustsbi/sbi-spec/compare/v0.0.7...HEAD
[0.0.7]: https://github.com/rustsbi/sbi-spec/compare/v0.0.6...v0.0.7
[0.0.6]: https://github.com/rustsbi/sbi-spec/compare/v0.0.5...v0.0.6
[0.0.5]: https://github.com/rustsbi/sbi-spec/compare/v0.0.4...v0.0.5
[0.0.4]: https://github.com/rustsbi/sbi-spec/compare/v0.0.3...v0.0.4
[0.0.3]: https://github.com/rustsbi/sbi-spec/compare/v0.0.2...v0.0.3
[0.0.2]: https://github.com/rustsbi/sbi-spec/compare/v0.0.1...v0.0.2
[0.0.1]: https://github.com/rustsbi/sbi-spec/releases/tag/v0.0.1
