# RustSBI Prototyper

RustSBI Prototyper is RISC-V firmware that provides the Supervisor Binary Interface (SBI).
Runtime provides trap entry and hart services; Prototyper supplies platform drivers and SBI policy.

## Usage

### Required Dependencies

Install Rust through rustup; the repository's `rust-toolchain.toml` selects the toolchain,
components, and RISC-V targets.
Install the firmware build tools:

```bash
cargo install cargo-binutils
sudo apt install u-boot-tools
```

### Firmware Compilation

Run build commands from the repository root:

```bash
cargo prototyper build [OPTIONS] [MODE]
```

The default mode is `dynamic`.
Builds produce an ELF file and a raw binary under `target/<TARGET>/release/`.
The default target is `riscv64gc-unknown-none-elf`.
`--debug` selects the `debug/` directory instead.

| Command | Mode | Output basename |
| --- | --- | --- |
| `cargo prototyper build` or `cargo prototyper build dynamic` | Loader-supplied next-stage information | `rustsbi-prototyper-dynamic` |
| `cargo prototyper build jump` | Configured next-stage address | `rustsbi-prototyper-jump` |
| `cargo prototyper build payload <PATH>` | Embedded payload binary | `rustsbi-prototyper-payload` |

Each basename has `.elf` and `.bin` outputs.

#### Build Options

- `-f, --features <FEATURES>`
  Enable firmware features, for example `--features hypervisor`.
- `--no-default-features`
  Disable default firmware features.
- `--fdt <PATH>`
  Specify the path to a Flattened Device Tree (FDT) file.
- `-c, --config-file <PATH>`
  Specify the path to a custom configuration file.
- `--debug`
  Build with the `debug` profile instead of `release`.
- `--target <TARGET>`
  Override the target triple (default: `riscv64gc-unknown-none-elf`).
- `-v, --verbose`
  Increase runner logging verbosity.
- `-q, --quiet`
  Decrease runner logging verbosity.
- `-h, --help`
  Display help information.

Use `--fdt <PATH>` to embed a device tree as the platform-description source in any mode.
The loader must still provide writable destination storage as described below.

### Tests and Benchmarks

`cargo prototyper test` builds the test kernel and embeds it in
`rustsbi-prototyper-payload-test.{elf,bin}`.
`cargo prototyper bench` does the same for the benchmark kernel,
producing `rustsbi-prototyper-payload-bench.{elf,bin}`.
Both commands boot QEMU with `-machine virt -m 256M -nographic`
and check its console output.
They require `qemu-system-riscv64` on `PATH`;
on Ubuntu, install it with `sudo apt install qemu-system-misc`.

Both commands accept:

```text
[--pack] [--no-run] [--smp <N>] [--timeout <SECS>] [--retries <N>]
[--debug] [-c|--config-file <PATH>]
```

`--no-run` builds without starting QEMU.
`--debug` and `--config-file` select the firmware build profile and configuration.
`--retries` sets the total number of QEMU attempts; only timeouts trigger another attempt.

| Command | Harts | Timeout per attempt | Total attempts |
| --- | --- | --- | --- |
| `test` | 1 | 60 seconds | 2 |
| `bench` | 4 | 90 seconds | 4 |

`--pack` also builds `rustsbi-prototyper-dynamic-pack.{elf,bin}`
and packs it with the kernel into `rustsbi-{test,bench}-kernel.itb` for U-Boot SPL.
It preserves existing `rustsbi-prototyper-dynamic.{elf,bin}` outputs.
QEMU verifies the payload image only.
Boot the ITB through U-Boot SPL to test the packed image.

### Configuration File

Customize firmware parameters in `firmware/prototyper/config/default.toml`,
or select another file with `--config-file`.
For example:

```toml
num_hart_max = 8
stack_size_per_hart = 16384  # 16 KiB (16 * 1024)
heap_size = 32768            # 32 KiB (32 * 1024)
page_size = 4096             # 4 KiB
log_level = "INFO"
link_start_address = 0x80000000
payload_address = 0x80200000
jump_address = 0x80200000
tlb_flush_limit = 16384      # 16 KiB (page_size * 4)
```

#### Configuration Options

- `num_hart_max`: Maximum enabled hart count accepted at boot, defaulting to 8.
  Hardware IDs may be sparse: IDs 0 and 4 need capacity 2.
  Runtime establishes one compact mapping shared by stack selection and per-hart software state;
  there is no fixed eight-hart limit in Runtime.
- `stack_size_per_hart`: Boot/trap stack size per enabled hart, in bytes.
  It must be a multiple of 128 and defaults to 16384 (16 KiB) when omitted.
  Choose a size that accommodates the firmware's boot and trap call paths.
  Changing the hart capacity or stack size does not recompile Runtime.

  One stack is linked for serialized discovery and then retained by the boot hart.
  The other stacks follow the linked image according to the discovered hart count.
  The loader must provide exclusive RAM there, outside the complete next-stage image and other live objects.
  Boot checks RAM, reservations, DTB and handoff ranges.
  The resulting firmware reservation and PMP boundary include all stacks.
- `heap_size`: Linker-reserved firmware heap size in bytes, at least 32.
  Prototyper uses a first-fit allocator;
  Runtime owns the heap storage and global allocation boundary.
- `page_size`: Page size, in bytes.
- `log_level`: Logging level (`TRACE`, `DEBUG`, `INFO`, `WARN`, `ERROR`).
- `link_start_address`: Address where the firmware itself is linked and loaded.
- `payload_address`: Address where payload-mode firmware loads and jumps to the payload.
- `jump_address`: Target address for jump mode.
- `fdt_handoff_address`, `fdt_handoff_capacity`: Optional paired DTB destination
  address (nonzero, 8-byte aligned) and writable capacity in bytes.
  Without them, the loader must provide an incoming DTB at `a1`
  with its initial `totalsize` plus 1024 writable bytes.
  This also applies when `--fdt` selects an embedded source.
  The capacity is a loader precondition; RustSBI does not measure it.

  The destination must be exclusively writable and outside firmware and other live objects.
  It must remain valid until the next stage finishes using it.
  Configuring a range does not allocate it.
- `tlb_flush_limit`: TLB flush limit, in bytes.

Custom configuration files must define `link_start_address`, `payload_address`, `jump_address`, and `heap_size`.
These addresses must be 0x1000-aligned, and `link_start_address` must be lower than `payload_address`.

To use a custom configuration file, specify it with:

```bash
cargo prototyper build -c /path/to/custom_config.toml
```

### Running an Example

Run dynamic firmware with a supervisor kernel in QEMU:

```bash
qemu-system-riscv64 \
  -machine virt \
  -bios target/riscv64gc-unknown-none-elf/release/rustsbi-prototyper-dynamic.elf \
  -kernel /path/to/supervisor-kernel.bin \
  -display none \
  -serial stdio
```

For platform and debugging instructions, see the [Prototyper guide](../docs/src/chapter_03_prototyper.md).

## Setting Up the Development Environment

### Required Packages

Install the packages listed under [Required Dependencies](#required-dependencies).

### Optional Development Tools

#### pre-commit

A tool to run code checks before committing:

```bash
pipx install pre-commit
pre-commit install  # Set up pre-commit for the project
```

#### Cargo Deny

A Cargo plugin to audit dependency security:

```bash
cargo install --locked cargo-deny
```

#### typos

A spell-checking tool for code and documentation:

```bash
cargo install typos-cli
```

#### git-cliff

A changelog generation tool:

```bash
cargo install git-cliff
```

## License

This project is dual-licensed under MIT or Mulan-PSL v2.
See [LICENSE-MIT](../LICENSE-MIT) and [LICENSE-MULAN](../LICENSE-MULAN) for details.
