# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

* PMP programming uses the hart's configuration-register layout, verifies the effective permissions and range, and synchronizes address translations. Hardware writes require an unsafe caller.
* PMP address decoding represents ranges wider than XLEN. The slot allocator supports RV32 and rejects indices outside its managed entries.
* `set_pmp_entry` calls the platform's local translation synchronization operation after programming or rollback. Rollback preserves address bits hidden by OFF or TOR on coarse-grained harts. Platforms handle all-hart updates. Address-only writes can prepare a NAPOT encoding while an entry is OFF.
