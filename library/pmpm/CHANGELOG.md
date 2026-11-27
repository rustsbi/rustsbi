# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed

* PMP slot indices are checked by the `PmpSlot` type; out-of-range slots are no longer accepted.
* Zero-length NAPOT returns an error instead of panicking.
* TOR entries are encoded with their exclusive top address, and writes that would move another region are refused.
* RV32 uses `pmpcfg1` and `pmpcfg3` for entries 4-7 and 12-15.

### Changed

* **Breaking:** PMP slot APIs take `PmpSlot`. `PMPSlotAllocator::reserve` takes a slot out of allocation. `decode_pmp_addr` for TOR returns the top address through `hi()`.
