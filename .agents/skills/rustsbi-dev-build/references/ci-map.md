<!-- SPDX-License-Identifier: MulanPSL-2.0 -->

# CI coverage

The recipes follow `Library.yml` and `prototyper.yml` at firmware revision
`c13891d08ddd6acbaaa40b21fcb0a40b983b39aa`.
When those jobs or xtask interfaces change, compare the current workflow before
changing the corresponding recipe in `scripts/rustsbi-build-cli/src/plan.rs`.
Each planned step records its source workflow.

| Suite | Repository source and behavior |
|---|---|
| `build` | `Cargo.toml` host default members, then `cargo prototyper build dynamic` |
| `library` | `Library.yml`: fmt; rustsbi default/machine/forward/machine+forward on RV32 and RV64; sbi-rt and sbi-testing default/log target builds; rustsbi default/machine and riscv-aia/sbi-spec/rpmi/penglai/sbi-rt host tests |
| `prototyper` | `prototyper.yml/rust-clippy-analyze`: fmt, xtask tests, generated dynamic firmware inputs, Prototyper/runtime-macro/runtime host tests, Clippy for firmware/test/bench target triples |
| `qemu` | `prototyper.yml/qemu-boot-tests`: dynamic and jump firmware, both kernels, then six payload/dynamic/jump × test/bench boots using xtask and `.github/scripts/prototyper-qemu-boot.sh` |
| `pack` | `cargo prototyper test --pack --no-run` and `bench --pack --no-run`; require both nonempty ITBs |
| `all` | Union of library, prototyper, qemu and pack, sharing prerequisite builds |

Direct Cargo library/test/Clippy commands use `--locked`; xtask is invoked
through the repository's locked Cargo alias.
The `forward` feature is tested on RISC-V, never as a host test.
Firmware checks run after xtask has generated the inputs consumed by build.rs.
Clippy emits ordinary local diagnostics instead of requiring SARIF upload tools.
Packing creates ITBs, while QEMU verification uses the firmware/kernel boot
paths above; it does not boot the ITB through U-Boot/SPL.

The local runner executes serially and stops on the first failure.
GitHub's parallel jobs, cache/artifact actions, SARIF uploads, dependency
installation, DCO/changelog checks, stable/MSRV library matrix, special FDT
handoff/M-mode interrupt and ACLINT probes and downstream OS integrations are outside
this version's `all` suite.
Those exclusions keep an ordinary local build bounded and prevent it from
silently provisioning toolchains or external operating-system trees.
Use the corresponding repository workflow/script directly when those tests
are part of the requested scope; do not describe `all` as every GitHub CI job.
