---
# SPDX-License-Identifier: MulanPSL-2.0
name: rustsbi-qemu-dev-setup
description: Use when checking or setting up a Linux host for RustSBI QEMU development on Arch or Debian. Inspect prerequisites without installing anything, then prepare the environment only after approval of its write inventory.
license: MulanPSL-2.0
---

# RustSBI QEMU development setup

Check a Linux development host and, when authorized, prepare RustSBI and QEMU.
The same scripts support Arch Linux and Debian without calling a system package
manager or sudo.
Native prerequisites and manual preparation examples are in [README.md](README.md).
The design and verification scope are in [spec/README.md](spec/README.md).

## Procedure

1. Resolve this skill's absolute directory from its installed location.
   Run `bash <absolute-skill-dir>/scripts/preflight.sh --repo-dir <absolute-repo-dir>`.
   Preflight only inspects Rust command paths; it never executes Rust proxies.
   Exit `0` means host commands are present, `2` means setup actions are needed,
   and `3` means native prerequisites need manual preparation.
   Repository toolchain synchronization is deferred to the approved setup phase.
2. If setup is requested, use `setup.sh --plan` with the intended options.
   Report the repository, source input, QEMU selection and prefix, Rust homes,
   cargo-binutils root, temporary directories and build outputs.
   Explain any required native preparation using the appropriate Arch or Debian
   instructions; do not install system packages implicitly.
3. Obtain explicit approval for that inventory before running `setup.sh --yes`.
   Existing approval for the same inventory remains valid.
   `--yes` acknowledges the shell operation; it does not establish human approval.
   Do not chain setup into another skill or CI job without approval for that use.
4. Execute the approved command and report its exit status.
   Reuse an existing RustSBI workspace only when no `--local-copy` was supplied.
   A copy or clone requires an absent destination; existing paths and symlinks
   are refused, including paths that appear while source staging is in progress.
5. Report the actual QEMU binary, firmware paths and `boot_verified` result.
   `--no-test` reports `boot_verified: 0` and proves only the build.
   A failed build, installation audit or boot test is a failed setup.

## Write inventory

| Operation | Write locations and boundaries |
|---|---|
| Source preparation | New repository and a temporary sibling staging directory; no merge into existing directories |
| QEMU build | New temporary source/build/cache directories under `TMPDIR` and a staging directory beside the selected prefix |
| QEMU installation | Entire payload, including data, headers/libraries and `install-manifest.txt`, below the selected new prefix; staged entries outside it abort publication |
| Rust installation | Actual `RUSTUP_HOME` and `CARGO_HOME` or their standard defaults; missing toolchains/components, rustup proxies and registry caches |
| cargo-binutils | Missing tools in the reported private cargo-tools root; Cargo caches in `CARGO_HOME` |
| Firmware build/test | Selected checkout's `target/` and temporary runtime files |

QEMU defaults to `~/.local/share/rustsbi/qemu/10.2.1`.
Tools, guest agent, plugins and firmware blobs are disabled in its source build.
The staged manifest is printed before atomic publication; existing prefixes are
never replaced.
The script changes PATH only for its own process and uses rustup's
`--no-modify-path` option.
Temporary cleanup is limited to directories/files created by the current run.

## QEMU selection

`auto` reuses a usable QEMU on PATH or builds into the private prefix.
`distro` only reuses PATH; it does not invoke a package manager.
`source` selects the requested source version or reuses an existing private
installation of that version.
All modes require QEMU >= 9.1 and reject 10.2.4, which failed the recorded
SPVP test.
10.2.1 is the validated default, not a permanent upper bound or a CI package pin.
Other versions require their own boot verification.

Use `--local-copy` for a local RustSBI directory or tar archive, and
`--qemu-archive` for an already downloaded QEMU source release.
See `setup.sh --help` for the complete interface.
