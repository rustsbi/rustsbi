---
name: rustsbi-qemu-dev-setup
description: Use when preparing or diagnosing a RustSBI QEMU development environment on Linux, including Arch and Debian. Inspect prerequisites and planned writes before provisioning a selected checkout and privately installed tools.
license: MulanPSL-2.0
---

# RustSBI development setup

Prepare the selected Linux checkout, build dynamic firmware and verify its
QEMU boot. Native prerequisites are supplied by the host; this skill never
runs a package manager or edits shell startup files.
Read [README.md](README.md) for bootstrap and native dependencies, and
[spec/README.md](spec/README.md) for write boundaries and validation.

## Interface

All deterministic logic lives in one standalone Rust workspace,
`scripts/rustsbi-setup-cli`. The shell entrypoints only dispatch its subcommands.
Resolve this skill's absolute directory for every command.

```sh
bash "$SKILL/scripts/preflight.sh" --repo-dir /work/rustsbi
bash "$SKILL/scripts/setup.sh" --repo-dir /work/rustsbi --plan
bash "$SKILL/scripts/setup.sh" --repo-dir /work/rustsbi --yes
```

Use the prebuilt release binary, or set `RQS_CLI` to its absolute path.
Preflight and plan never build the helper or execute Rust proxies.
When the host has no Rust installation, carry a compatible CLI binary from
a prepared machine; setup then bootstraps rustup with no default toolchain
and resolves the checkout's pinned toolchain during the build.

| Option | Meaning |
|---|---|
| `--repo-dir DIR` | Existing workspace or absent destination; default `$HOME/rustsbi` |
| `--local-copy PATH` | Copy a local directory or `.tar`, `.tar.gz`, `.tar.xz` archive to an absent destination |
| `--qemu-from auto\|distro\|source` | Reuse suitable QEMU, require PATH QEMU, or build privately |
| `--qemu-version VER` | Source version, default `10.2.1` |
| `--qemu-prefix DIR` | Private prefix, default `$HOME/.local/share/rustsbi/qemu/VER` |
| `--qemu-archive PATH` | Local source archive instead of downloading QEMU |
| `--no-test` | Compile without boot verification |
| `--plan` | Print write inventory without provisioning |
| `--yes` | Acknowledge the displayed provisioning writes |

## Procedure

1. Inspect the selected repository and existing work. Run preflight first.
   Exit `0` means host tools are present, `2` means setup can supply missing
   tools, `3` means native prerequisites are missing, and `1` is invalid input
   or a failed probe. Repository toolchain readiness is checked during setup.
2. Present the write inventory and obtain authorization for provisioning.
   A clear user request to provision this environment already authorizes it;
   reuse that authorization. Read-only diagnosis alone does not authorize setup.
3. Run setup with the same options and `--yes`. Preserve existing workspaces;
   a local copy requires an absent destination. On a failed command inspect
   the concrete failure before retrying. Select a new private prefix or
   destination if a conflicting path exists.
4. Report the repository, QEMU binary, firmware artifacts and actual boot
   result. `boot_verified: true` requires a successful `cargo prototyper test`.
   With `--no-test`, report compilation only.

The process adds tool paths only to child processes. Subsequent shells must
explicitly add the reported QEMU, Cargo and cargo-binutils bin directories
to PATH. Do not claim persistent shell configuration was installed.
