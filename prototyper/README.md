# RustSBI Prototyper

RustSBI Prototyper is a developing RISC-V Secure Bootloader solution. It can be integrated with the Rust or C language ecosystem to form a complete RISC-V bootloader ecosystem.

## Usage

### Basic Usage

#### Required Dependencies:

Before compiling, ensure the following packages are installed:

```bash
cargo install cargo-binutils
sudo apt install u-boot-tools
```

These are necessary for building the firmware and handling RISC-V binary outputs.

#### Compilation Command:

The following command compiles the RustSBI Prototyper bootloader with optional settings:

```bash
cargo prototyper build [OPTIONS] [COMMAND]
```

The firmware mode is selected by an optional subcommand: `dynamic` (the default when no subcommand is given), `jump`, or `payload <PATH>`. The resulting files are generated in the `target/riscv64gc-unknown-none-elf/release/` directory under your project root.

#### Commands

- `cargo prototyper build [dynamic]`
  Build dynamic firmware (default when no subcommand is given).
- `cargo prototyper build jump`
  Build jump-mode firmware.
- `cargo prototyper build payload <PATH>`
  Build payload-mode firmware embedding the given payload binary.
- `cargo prototyper test [--pack] [--no-run] [--smp <N>] [--timeout <SECS>] [--retries <N>] [--debug] [-c|--config-file <PATH>]`
  Build the test kernel and payload-mode firmware embedding it (`rustsbi-prototyper-payload-test.{elf,bin}`), then boot the firmware in QEMU and verify the kernel output. Requires `qemu-system-riscv64` on `PATH` (e.g., `sudo apt install qemu-system-misc`); pass `--no-run` to only build. `--debug` and `--config-file` are forwarded to the firmware build.
- `cargo prototyper bench [--pack] [--no-run] [--smp <N>] [--timeout <SECS>] [--retries <N>] [--debug] [-c|--config-file <PATH>]`
  Build the bench kernel and payload-mode firmware embedding it (`rustsbi-prototyper-payload-bench.{elf,bin}`), then boot the firmware in QEMU and verify the kernel output. QEMU options work the same as for `test` (defaults: `--smp 4 --timeout 90 --retries 4`); `--debug` and `--config-file` are forwarded to the firmware build.

#### Options (on `cargo prototyper build`)

- `-f, --features <FEATURES>`
  Enable specific features during the build (supports multiple values, e.g., `--features "hypervisor,feat2"`).
- `--fdt <PATH>`
  Specify the path to a Flattened Device Tree (FDT) file.
- `-c, --config-file <PATH>`
  Specify the path to a custom configuration file.
- `--debug`
  Build with the `debug` profile instead of `release`.
- `--target <TARGET>`
  Override the target triple (default: `riscv64gc-unknown-none-elf`).
- `-v, --verbose`
  Increase logging verbosity (more detailed output).
- `-q, --quiet`
  Decrease logging verbosity (less output).
- `-h, --help`
  Display help information.

> #### Note on FDT Files
>
> Regardless of the mode (Dynamic Firmware, Payload Firmware, or Jump Firmware), specifying an FDT file with `--fdt` ensures it is used to initialize the hardware platform configuration. The FDT file provides essential hardware setup details and overrides the bootloader's default settings.

### Firmware Compilation

#### 1. Dynamic Firmware

**Compilation Command:**
Use this command to compile firmware that dynamically loads payloads:

```bash
cargo prototyper build
```

**Output:**
Once compiled, the firmware files will be located in the `target/riscv64gc-unknown-none-elf/release/` directory under your project root:
- `rustsbi-prototyper-dynamic.elf` (ELF executable)
- `rustsbi-prototyper-dynamic.bin` (Binary file)

#### 2. Payload Firmware

**Compilation Command:**
Build firmware with an embedded payload:

```bash
cargo prototyper build payload <PAYLOAD_PATH>
```

**Output:**
After compilation, the resulting firmware files are generated in the `target/riscv64gc-unknown-none-elf/release/` directory:
- `rustsbi-prototyper-payload.elf`
- `rustsbi-prototyper-payload.bin`

`cargo prototyper test` and `cargo prototyper bench` are shorthands that build the test/bench kernel and embed it as the payload, producing `rustsbi-prototyper-payload-test.{elf,bin}` and `rustsbi-prototyper-payload-bench.{elf,bin}` respectively. By default they also boot the firmware in QEMU (`-machine virt -m 256M -nographic`) and check the console output for the expected test results; use `--no-run` to skip the QEMU run, and `--smp`/`--timeout`/`--retries` to tune it. The firmware build itself defaults to release mode with the default config; pass `--debug` to build it in the debug profile and `-c|--config-file <PATH>` to use a custom firmware configuration. With `--pack`, an additional dynamic-mode firmware is built and packed with the kernel into a combined ITB image (`rustsbi-{test,bench}-kernel.itb`) for U-Boot SPL boot flows; the QEMU run still verifies the payload-mode firmware.

#### 3. Jump Firmware

**Compilation Command:**
Build firmware for jump mode:

```bash
cargo prototyper build jump
```

**Output:**
After compilation, the resulting firmware files are generated in the `target/riscv64gc-unknown-none-elf/release/` directory:
- `rustsbi-prototyper-jump.elf`
- `rustsbi-prototyper-jump.bin`

### Configuration File

Customize bootloader parameters by editing `default.toml` located at `prototyper/prototyper/config/default.toml`. Example:

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

- `num_hart_max`: Maximum number of supported harts (hardware threads).
- `stack_size_per_hart`: Stack size per hart, in bytes.
- `heap_size`: Heap size, in bytes.
- `page_size`: Page size, in bytes.
- `log_level`: Logging level (`TRACE`, `DEBUG`, `INFO`, `WARN`, `ERROR`).
- `link_start_address`: Address where the firmware itself is linked and loaded.
- `payload_address`: Address where payload-mode firmware loads and jumps to the payload.
- `jump_address`: Target address for jump mode.
- `tlb_flush_limit`: TLB flush limit, in bytes.

Custom configuration files must define `link_start_address`, `payload_address`, and `jump_address`. Addresses must be 0x1000-aligned, and `link_start_address` must be lower than `payload_address`.

To use a custom configuration file, specify it with:

```bash
cargo prototyper build -c /path/to/custom_config.toml
```

### Running an Example

Run the generated firmware in QEMU:

```bash
qemu-system-riscv64 \
  -machine virt \
  -bios target/riscv64gc-unknown-none-elf/release/rustsbi-prototyper-dynamic.elf \
  -display none \
  -serial stdio
```

For additional examples, see the [docs](/prototyper/docs/) directory.

## Setting Up the Development Environment

### Required Packages

See the **[Required Dependencies](#required-dependencies)** under **Usage** above for the packages needed to compile the RustSBI Prototyper.

### Optional Development Tools

These tools are optional but recommended to enhance your development workflow:

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

This project is dual-licensed under MIT or Mulan-PSL v2. See [LICENSE-MIT](./LICENSE-MIT) and [LICENSE-MULAN](./LICENSE-MULAN) for details.
