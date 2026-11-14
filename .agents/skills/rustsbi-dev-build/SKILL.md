---
# SPDX-License-Identifier: MulanPSL-2.0
name: rustsbi-dev-build
description: Use when building a RustSBI checkout on Linux, running its local Library or Prototyper CI checks, or reproducing a build, Clippy, packing or QEMU boot failure.
license: MulanPSL-2.0
---

# RustSBI local build

Build the user's selected checkout and retain command logs and verified results.
Use the repository toolchain and existing CI/xtask entrypoints.
Read [references/ci-map.md](references/ci-map.md) when selecting coverage or
explaining how a local run differs from CI.

## Interface

Call this skill's absolute `scripts/run.sh` path with one quoted raw argument:

```text
plan|run SUITE [--repo DIR] [--qemu-bin FILE] OUTPUT_DIR
```

The CLI tokenizes the raw string; do not split it into shell arguments.
Quote paths containing spaces inside that string.
Paths resolve from the caller's current directory.
Use a new output directory for each invocation; existing paths are refused.

| Suite | Use for |
|---|---|
| `build` | Host libraries and default dynamic firmware |
| `library` | Library CI feature/target matrix and host tests |
| `prototyper` | Formatting, xtask/firmware tests and three Clippy targets |
| `qemu` | Test/bench kernels in payload, dynamic and jump modes |
| `pack` | Build test/bench ITB images without booting those images |
| `all` | All CI-derived suites above; see the coverage exclusions |

## Procedure

1. Resolve this skill's absolute directory and the checkout requested by the user.
   Use the current checkout for an ordinary local build request.
   Select the smallest suite that covers the requested work; `build` is the
   ordinary compile path, while `all` is for a requested broad CI check.
2. Use `plan` when the requested scope needs inspection before execution.
   Plan mode writes its report and may compile the helper; it does not execute
   the planned build commands.
   A clear request to build or test authorizes the corresponding `run` directly.
3. Invoke `run` with a fresh output directory.
   Build products use the checkout's `target/`; helper products use this skill's
   `target/`; Cargo may download dependencies into its normal cache.
   The runner disables rustup automatic installation and never runs a package
   manager, edits source files, applies formatting or changes Git state.
   If prerequisites are missing, report them. Use `rustsbi-qemu-dev-setup` only
   when environment preparation is separately requested or already authorized.
4. Inspect `report.json` and the first failed step's log.
   Missing prerequisites return `3`; failed commands or artifacts return `1`;
   invalid input or report I/O errors return `2`.
   Fix code only when the user's task also authorizes that change.
   For an authorized fix, rerun the relevant suite into a new output directory.
5. Report the suite, checkout revision/dirty state, result directory and failure
   details or verified artifacts.
   A passing build is not a passing boot test.
   `boot_verified` is true only after every selected boot step passed.
   Packing verifies nonempty ITBs; it does not prove a U-Boot/SPL ITB boot.

Do not start overlapping build commands in this checkout while a run holds its
lock. The lock serializes this runner; independent Cargo/xtask commands do not
participate in it.

See [README.md](README.md) for executable examples and prerequisite details,
and [spec/README.md](spec/README.md) for the execution/report design.
