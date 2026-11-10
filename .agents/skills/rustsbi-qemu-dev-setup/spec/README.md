<!-- SPDX-License-Identifier: MulanPSL-2.0 -->

# Portable setup and write boundaries

## Goals

Run the same setup on Arch Linux and Debian without invoking a distribution
package manager or requiring sudo.
Preserve existing files, keep preflight read-only, and bound every installation.
Boot verification remains the existing `cargo prototyper test` command.
Bootstrapping a C compiler or system development libraries is out of scope.

## Overview

Preflight inspects commands and files without executing Rust proxies.
Setup validates its arguments and destinations before preparing source code.
It reuses an adequate QEMU or builds one into a new private prefix.
Rust installation and builds happen only in the approved setup phase.

## Key design

- Native build prerequisites are capabilities, not package names.
  Arch and Debian package lists are manual preparation examples.
- Existing RustSBI workspaces may be reused explicitly.
  New copies and clones are staged beside their destination and published
  with Linux `renameat2(RENAME_NOREPLACE)`.
  Other existing paths, including symlinks, are refused.
- QEMU uses an absolute, versioned prefix.
  `DESTDIR` captures the installation, including headers and libraries.
  Publication requires every staged entry to lie within that prefix.
  An install manifest records the complete payload.
- QEMU tools, guest agent, plugins and firmware blobs are disabled.
  Extraction omits unused EDK II ROM sources with an external host-header link;
  all other archive members remain subject to Python's data filter.
  No existing prefix is replaced and no shell startup file is edited.
- Preflight checks Rust command paths without executing `cargo`, `rustc`,
  `rustup`, or cargo-binutils proxies.

## Execution

1. Parse and validate options, source paths, destinations and host prerequisites.
2. Report the repository, QEMU prefix, Rust homes and cargo-binutils root.
3. Require `--yes` before creating source, cache or installation files.
4. Prepare source and, when needed, build and stage QEMU.
5. Check the staged manifest and publish into an absent prefix.
6. Install missing Rust tools, build firmware and optionally verify its boot.
7. Report the absolute QEMU path and whether boot verification ran.

The skill requires human approval of the reported changes.
`--yes` is the shell interface acknowledgement, not a replacement for that approval.

## Components

`common.sh` owns host prerequisites, options and QEMU selection.
`files.py` owns source staging, archive validation and atomic publication.
`preflight.sh` reports read-only capability checks.
`setup.sh` performs the approved writes.
The helpers remain local to this package; a broader CLI migration is separate work.

## Verification

Regression tests exercise existing-file preservation, destination races,
read-only Rust detection, rejected installation escapes and private-prefix
publication through the actual helper interfaces.
Command stubs make unintended package-manager, sudo and Rust proxy execution fail.
The same suite runs on Arch and Debian, including without root privileges.
Source-build smoke tests use the upstream QEMU release and RustSBI boot test.

Recorded validation on 2026-11-11 used x86_64 containers and an unprivileged user:

| Host | Regression suite | QEMU source install and manifest audit | RustSBI boot |
|---|---|---|---|
| Arch Linux rolling, base image 20261108.0.623074 | 20/20 passed | Passed | Passed |
| Debian 13 (trixie) | 20/20 passed | Passed | Passed |

Both hosts built QEMU 10.2.1 and firmware revision
`ebc7190fb630611fa9504f9f71089ce4b81c6c0e`, with
`SBI tests completed: PASS` and `boot_verified: 1`.
The QEMU release archive SHA-256 was
`a3717477d8e2c84d630bfffbc20f6cd3293eb45aa1e6dac6d0cc27689991c9e1`.
The installed files and directories matched each recorded manifest exactly.
Real Rust proxies with an absent pinned toolchain also left preflight and plan
snapshots unchanged on both hosts.

Native dependencies were prepared separately in the disposable containers.
Rust toolchains and Cargo caches were seeded from an existing installation;
missing crate archives were fetched during setup.
These runs cover QEMU source installation, firmware builds and boot verification,
but do not constitute a cold rustup installer test.

## Implementation sequence

Add the shared checks and staging helper, replace package-manager branches,
update the approval inventory, then run regression and distribution smoke tests.
Commit the package and focused tests together after checking the staged diff.
