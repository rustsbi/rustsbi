# Changelog

All notable changes to this project will be documented in this file. See [conventional commits](https://www.conventionalcommits.org/) for commit guidelines.

---

## Unreleased

### Added
- Add AIA IMSIC IPI backend support for RustSBI Prototyper.
- Add SpacemiT K1 SoC platform support for RustSBI Prototyper, including OrangePi RV2 board configuration.

### Modified
- fix(xtask): drain QEMU stdout/stderr on reader threads while the child runs, fixing a pipe-buffer deadlock on verbose kernel output
- fix(xtask): resolve repo-internal paths from the workspace root instead of the working directory, and honor `CARGO_TARGET_DIR` for firmware/kernel artifact directories
- fix(xtask): match `panicked` instead of `panic` in QEMU output verification to avoid false positives, and validate `--smp`/`--retries` before building (also with `--no-run`)
- feat(xtask): run test/bench kernels in QEMU after `cargo prototyper test`/`bench` builds, with `--no-run`, `--smp`, `--timeout`, and `--retries` options
- fix(xtask): pack test/bench ITB images with a dynamic-mode firmware instead of double-embedding the kernel
- refactor(xtask): unify prototyper build pipeline
- ci(prototyper): migrate workflow commands
- docs(prototyper): update build instructions
- fix(prototyper): handle build argument edge cases
- deps: update `sbi-spec` to version 0.0.10.
- test-kernel: update PMU flag parameter trait names.
- Refine CSR group comments.
- fix(prototyper): temporary PMU fix for possible S-mode DTB modification
- fix(prototyper): validate DBCN console shared memory range

### Removed
