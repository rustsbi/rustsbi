# Running the setup helper

Build on a prepared Linux machine with an installed Rust host toolchain
(Rust 1.88 or newer):

```sh
RUSTUP_AUTO_INSTALL=0 cargo build --release --locked \
  --manifest-path "$SKILL/scripts/rustsbi-setup-cli/Cargo.toml"
```

The executable is `scripts/rustsbi-setup-cli/target/release/rustsbi-setup-cli`.
Set `RQS_CLI` when using a separately built binary. For a host without Rust,
copy that executable from a compatible Linux host; preflight never compiles
it or installs a toolchain. An executable built on a newer libc may require
an older build host or a compatible static build for distribution.

## Native prerequisites

Every path needs Git, curl, a C compiler, CA certificates and a normal Linux
userspace. A private QEMU source build also needs a C++ compiler, make,
Ninja, Meson, pkg-config and GLib development headers. QEMU's configure
script validates its own additional build dependencies. Install those native
tools using the host's normal administration process before running setup.
The helper is distribution independent and never invokes apt, pacman or sudo.

On Debian, native tools normally come from `build-essential`, `git`, `curl`,
`ca-certificates`, `ninja-build`, `meson`, `pkg-config` and `libglib2.0-dev`.
On Arch, the corresponding packages are `base-devel`, `git`, `curl`,
`ca-certificates`, `ninja`, `meson`, `pkgconf` and `glib2`.
`device-tree-compiler`/`dtc` and `u-boot-tools` are optional for setup but
required by the build skill's packed-image suite.

## Examples

```sh
bash "$SKILL/scripts/preflight.sh" --repo-dir /work/rustsbi
bash "$SKILL/scripts/setup.sh" --repo-dir /work/rustsbi --plan
bash "$SKILL/scripts/setup.sh" --repo-dir /work/rustsbi --yes
```

To avoid a network clone, use `--local-copy /sources/rustsbi` or a tar archive.
The destination must not exist. Without a local source, an existing valid
workspace is reused; otherwise setup clones the official upstream repository.
`--qemu-from distro` only reuses PATH QEMU. `auto` reuses a suitable emulator
or builds `10.2.1` into a private prefix. Minimum version is `9.1.0`;
`10.2.4` is excluded because of the SPVP regression observed in boot tests.

Missing rustup is installed with `--default-toolchain none --no-modify-path`;
the selected checkout then resolves its own pinned toolchain and components.
Missing cargo-binutils `0.4.0` is installed under
`$HOME/.local/share/rustsbi/cargo-tools`.
`RUSTUP_HOME` and `CARGO_HOME` overrides are respected.

For validation, run the same fmt, Clippy, build and test commands as
`.github/workflows/Skills.yml` in the Rust workspace. Integration tests use
isolated command fixtures; real provisioning and boot evidence belongs
outside the deliverable package.
