# Changelog

All notable changes to this project will be documented in this file. See [conventional commits](https://www.conventionalcommits.org/) for commit guidelines.

---

## Unreleased

### Added
- Add DBCN contract tests with deferred error reporting to the test kernel.
- Add RV32 support to RustSBI Prototyper and test-kernel while preserving RV64 support.
- Add Yuzuki Neko board configuration with an RV32 firmware jump target of `0x40000000`.
- Add SBI Collaborative Processor Performance Control extension support to RustSBI Prototyper.
- Add SBI Message Proxy extension support to RustSBI Prototyper.
- Add SBI Steal-time Accounting extension support to RustSBI Prototyper.
- Add SBI Debug Triggers extension support to RustSBI Prototyper.
- Add SBI Firmware Features extension support to RustSBI Prototyper.
- Add SBI Nested Acceleration extension support to RustSBI Prototyper.
- Add SBI Supervisor Software Events scaffolding to RustSBI Prototyper; the extension remains
  unavailable until supervisor context switching is implemented.
- Add AIA IMSIC IPI backend support for RustSBI Prototyper.
- Add SpacemiT K1 SoC platform support for RustSBI Prototyper, including OrangePi RV2 board configuration.

### Modified
- Skip instruction fetch on SBI return and add batched per-call latency benchmarks with SBI console output.
- Generalize CCI-550, derive per-hart availability and fix K1 setup, I/O ordering and HSM restart with PMU wakeup.
- Remove K1-only initialization logging from platform publication.
- Reuse generic UART backends for K1 and preserve the baud divisor when its clock rate is unknown.
- Enable T-Head PLIC supervisor access and configure Svpbmt from the device tree.
- Clear T-Head MAEE for standard RV32 or Svpbmt page tables.
- Replace Prototyper IPI devices with fallible `IpiBackend` windows and SBI-layer target validation.
- Replace Prototyper console devices with fallible, non-blocking `DbcnBackend` slice operations.
- Replace Prototyper reset devices with typed requests and a fallible `ResetBackend` interface.
- Forward unsupported misaligned loads to S-mode so Linux vector alignment probes do not panic.
- refactor(prototyper): unify build commands (#227)
- deps: update `sbi-spec` to version 0.0.10.
- test-kernel: update PMU flag parameter trait names.
- Refine CSR group comments.
- fix(prototyper): temporary PMU fix for possible S-mode DTB modification
- fix(prototyper): validate DBCN console shared memory range

### Removed
