# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/), and this project adheres
to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Test new extension DBCN

### Modified

- Migrate sbi-rt crate to Rust 2024 edition.
- Update sbi-spec to version 0.0.7
- Update sbi-rt to version 0.0.3
- Rename `MArchId` and `MVendorId` into `MarchId` and `MvendorId` in `BaseCase`
- Replace `#[naked]` with `#[unsafe(naked)]` attribute in sbi-testing module to support stable Rust
    - Modified files:
        - `library/sbi-testing/src/thread.rs`
        - `library/sbi-testing/src/hsm.rs`
    - MSRV bumped to 1.88.0 ([PR #134213](https://github.com/rust-lang/rust/pull/134213))

### Fixed
- Fix typos.
- Avoid direct casting of function item into integers.

## [0.0.2] - 2023-01-20

### Modified

- Update dependency crate `riscv` to version 0.10.1
- Remove feature declaration `asm_sym`, bump MSRV to 1.66.0

## [0.0.1] - 2022-10-10

### Modified

- Project structure to keep test functions at root module
- Use `sbi-rt` v0.0.2 and `sbi-spec` v0.0.4

[Unreleased]: https://github.com/rustsbi/sbi-testing/compare/v0.0.2...HEAD
[0.0.2]: https://github.com/rustsbi/sbi-testing/compare/v0.0.1...v0.0.2
[0.0.1]: https://github.com/rustsbi/sbi-testing/compare/v0.0.0...v0.0.1

