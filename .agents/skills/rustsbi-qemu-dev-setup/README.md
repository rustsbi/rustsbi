<!-- SPDX-License-Identifier: MulanPSL-2.0 -->

# RustSBI QEMU development setup

Prepare a Linux host for RustSBI firmware builds and QEMU boot tests.
The runtime scripts do not use apt, pacman or sudo.
They reuse installed native tools and build missing QEMU into a private prefix.
Arch Linux and Debian use the same execution path.

## Host prerequisites

The base path needs glibc-based Linux, Bash, Python 3 with `tarfile.data_filter`,
Git, curl and a C compiler.
A QEMU source build additionally needs a C++ compiler, make, ninja, meson,
pkg-config, Python venv/ensurepip and GLib development files.
Missing native prerequisites stop setup before it changes files.
Preparing those prerequisites is a separate host administration step.

For a Debian development host, the relevant packages are:

```sh
sudo apt-get install build-essential git curl ca-certificates python3 python3-venv \
    pkg-config meson ninja-build libglib2.0-dev
```

For an already updated Arch development host:

```sh
sudo pacman -S --needed base-devel git curl ca-certificates python meson ninja \
    pkgconf glib2
```

These commands are manual examples, not steps run by the scripts.
Do not refresh Arch's package database and install selected packages without
keeping the system consistent; this setup never performs a partial upgrade.
`mkimage` and `dtc` are optional for the default boot test.
Install `u-boot-tools`/`device-tree-compiler` on Debian or `uboot-tools`/`dtc` on
Arch when packed images or device-tree work are needed.

## Quick start

From a RustSBI checkout, use absolute paths:

```sh
skill_dir="$(pwd)/.agents/skills/rustsbi-qemu-dev-setup"
bash "$skill_dir/scripts/preflight.sh" --repo-dir "$(pwd)"
bash "$skill_dir/scripts/setup.sh" --repo-dir "$(pwd)" --plan
```

Review the inventory and obtain approval, then run:

```sh
bash "$skill_dir/scripts/setup.sh" --repo-dir "$(pwd)" --yes
```

To prepare a separate source copy, choose a destination that does not exist:

```sh
bash "$skill_dir/scripts/setup.sh" --repo-dir "$HOME/rustsbi-dev" \
    --local-copy /absolute/path/to/rustsbi --yes
```

Existing RustSBI workspaces can be reused without `--local-copy`.
Other existing directories, empty directories, files and symlinks are refused.
Tar archives may contain the workspace at their root or inside one wrapper
directory; unsafe archive paths are rejected.

## QEMU and installation scope

The default source release is 10.2.1.
QEMU >= 9.1 is required; 10.2.4 is excluded after an observed SPVP boot failure.
This is not a claim about every intermediate or newer release.
An existing usable PATH binary can be reused with `--qemu-from distro`.
That mode does not install distribution packages.

Source builds use `DESTDIR` and a new private prefix, defaulting to
`~/.local/share/rustsbi/qemu/10.2.1`.
All installation paths are audited and recorded in `install-manifest.txt`.
Existing prefixes are not replaced.
Use `--qemu-prefix` to select a different absent path and `--qemu-archive` to
use a local source release.
No shell startup file is edited, and PATH changes last only for the setup process.
The output names the actual binary for subsequent commands.

## Results and checks

Preflight exits `0` when host commands are present, `2` for setup actions,
or `3` for missing native prerequisites.
Invalid arguments or unusable paths fail with exit `1`.
It never executes cargo/rustc/rustup proxies and does not synchronize toolchains.
Setup builds the host libraries and dynamic firmware, then runs
`cargo prototyper test` unless `--no-test` was selected.
Only a successful boot test produces `boot_verified: 1`.

`cargo prototyper test --pack` creates an ITB and runs the payload ELF test;
it does not verify booting that ITB through U-Boot/SPL.

Run `make check` for syntax and `make test` for isolated regression tests.
See [spec/README.md](spec/README.md) for design boundaries and test scope,
and [SKILL.md](SKILL.md) for the agent procedure and full write inventory.
