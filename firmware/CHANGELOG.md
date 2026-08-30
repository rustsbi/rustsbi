# Changelog

All notable changes to this project will be documented in this file. See [conventional commits](https://www.conventionalcommits.org/) for commit guidelines.

---

## Unreleased

### Added
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
- Add SpacemiT K3 board configuration, warm entry, cache coherency,
  power-domain control, and protected RCPU runtime regions.
- Add K3 access-fault emulation, hart wake-up, system suspend, and IMSIC state
  preservation hooks.
- Add the shared-memory RPMI mailbox transport used by K3 platform services.
- Provide RPMI-backed CPPC and MPXY services when the platform mailbox is
  present. MPXY exposes only service groups without a dedicated SBI extension.
- Support FWFT feature locking and the SBI v3.0 `menvcfg` field layout and
  pointer-masking values.

### Modified
- refactor(prototyper): unify build commands (#227)
- deps: update `sbi-spec` to version 0.0.10.
- test-kernel: update PMU flag parameter trait names.
- Refine CSR group comments.
- fix(prototyper): temporary PMU fix for possible S-mode DTB modification
- fix(prototyper): validate DBCN console shared memory range

### Removed
