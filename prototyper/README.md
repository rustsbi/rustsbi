# RustSBI Prototyper

RustSBI Prototyper is an experimental RISC-V firmware implementation. It builds
dynamic, payload, and fixed-address firmware images for supported platforms.

## Prerequisites

On Debian or Ubuntu, install the host tools with:

```bash
sudo apt install qemu-system-misc u-boot-tools
cargo install cargo-binutils
```

The build also requires the Rust toolchain selected by this repository.

## Building firmware

Run the build command from the repository root:

```bash
cargo prototyper [OPTIONS]
```

The most commonly used options are:

- `-f, --features <FEATURES>` enables a comma-separated Cargo feature set.
- `--fdt <PATH>` embeds the supplied flattened device tree.
- `--payload <PATH>` builds a payload image containing the supplied ELF file.
- `--jump` builds a fixed-address jump image.
- `-c, --config-file <PATH>` selects a custom configuration file.
- `-v, --verbose` and `-q, --quiet` adjust build output.

Use `cargo prototyper --help` for the complete command-line reference.

### Dynamic image

```bash
cargo prototyper
```

This produces `rustsbi-prototyper-dynamic.elf` and
`rustsbi-prototyper-dynamic.bin` in the target release directory.

### Payload image

```bash
cargo prototyper --payload <PAYLOAD_ELF>
```

This produces `rustsbi-prototyper-payload.elf` and
`rustsbi-prototyper-payload.bin`.

### Jump image

```bash
cargo prototyper --jump
```

This produces `rustsbi-prototyper-jump.elf` and
`rustsbi-prototyper-jump.bin`.

Supplying `--fdt <PATH>` in any mode uses that device tree as the firmware's
hardware description.

## Configuration

The default configuration is
[`prototyper/config/default.toml`](prototyper/config/default.toml). A custom
configuration can be selected with:

```bash
cargo prototyper --config-file /path/to/custom.toml
```

The configuration controls hart and memory limits, DTB validation bounds,
logging, the fixed jump address, and the TLB-flush threshold. Keep sizes and
address ranges consistent with the target platform.

## Running in QEMU

For a basic dynamic-image boot:

```bash
qemu-system-riscv64 \
  -machine virt \
  -bios target/riscv64gc-unknown-none-elf/release/rustsbi-prototyper-dynamic.elf \
  -display none \
  -serial stdio
```

More complete examples are available in the [boot guides](docs/).

## Development checks

Run the ordinary Rust checks before submitting a change:

```bash
cargo fmt --all -- --check
cargo test -p rustsbi-prototyper-machine
cargo test -p rustsbi-prototyper
cargo test -p xtask
```

Install the optional repository checks as needed:

```bash
pipx install pre-commit
pre-commit install
cargo install --locked cargo-deny
cargo install typos-cli
cargo install git-cliff
```

## License

This project is dual-licensed under the
[MIT license](../LICENSE-MIT) or the
[Mulan PSL v2](../LICENSE-MULAN).
