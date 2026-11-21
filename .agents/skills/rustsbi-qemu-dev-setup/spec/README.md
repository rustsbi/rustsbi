# Setup design and verification

## Goals

Provision a RustSBI QEMU workspace on Linux, including Arch and Debian,
without embedding a distribution package manager. Preserve existing sources,
keep QEMU private and distinguish compilation from verified boot.

## Overview

One standalone Rust CLI owns option parsing, prerequisite inspection,
source/archive validation, staged publication, command execution and verdicts.
`preflight` is read-only. `setup --plan` prints intended writes. Authorized
`setup --yes` provisions tools, builds firmware and optionally boots QEMU.
The two shell entrypoints only locate and execute that binary.

## Key design

- Preflight examines Rust proxy paths without executing them. A missing CLI
  is reported without compiling it. A prebuilt binary supports a Rust-free host.
- Existing repositories are reused only without `--local-copy`. New sources
  are staged beside the destination and published with Linux
  `renameat2(RENAME_NOREPLACE)`, so a concurrent writer is preserved.
- Archive extraction rejects traversal, escaping links and special files.
  Unused QEMU `roms/edk2` sources are skipped because they contain absolute
  host header links. No firmware ROM is built from them.
- QEMU uses a private prefix, including sysconfdir and localstatedir.
  `DESTDIR` stages the installation beside that prefix. Every staged entry
  is audited, including headers and libraries, before writing a manifest
  and atomically publishing it. Unmanaged existing prefixes are rejected.
- Source/build/cache temporaries use owned temporary directories.
  Rust downloads use the selected Rust homes. Shell startup files and system
  package databases are outside the write set.

## Flow

Validate inputs → inspect native tools/QEMU → inventory → authorize → stage
sources → build/reuse QEMU → bootstrap Rust when missing → resolve repository
toolchain → install missing objcopy → `cargo build --locked` →
`cargo prototyper build` → require nonempty ELF and BIN → optional boot test.
Any failed subprocess or missing artifact prevents a success verdict.

## Components

`scripts/rustsbi-setup-cli/src/main.rs` owns provisioning and subprocesses.
`src/files.rs` owns filesystem boundaries. Unit and integration tests live
in the same standalone workspace and are discovered by Skills CI.

## Validation

Exercise destination collisions and races, recursive local copies, unsafe
archives, staged-prefix escapes, no-write planning, Rust proxy avoidance,
missing firmware and successful fixture execution. Real acceptance must
also use a fresh container with no Rust or QEMU caches, record native
prerequisites separately, and retain full setup/build/boot logs externally.
Older acceptance records describe their tested revision only; rerun after
changing the deterministic implementation.
