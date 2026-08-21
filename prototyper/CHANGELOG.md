# Changelog

All notable changes to this project will be documented in this file. See [conventional commits](https://www.conventionalcommits.org/) for commit guidelines.

---

## Unreleased

### Added
- Add AIA IMSIC IPI backend support for RustSBI Prototyper.
- Add SpacemiT K1 SoC platform support for RustSBI Prototyper, including OrangePi RV2 board configuration.
- Add SpacemiT K3 SoC platform support for RustSBI Prototyper, including K3 board configurations (Pico-ITX, CoM260, CoM260 IFX and DeepComputing FML13V05).

### Modified
- deps: update `sbi-spec` to version 0.0.10.
- test-kernel: update PMU flag parameter trait names.
- Refine CSR group comments.
- fix(prototyper): temporary PMU fix for possible S-mode DTB modification
- fix(prototyper): validate DBCN console shared memory range
### Removed
