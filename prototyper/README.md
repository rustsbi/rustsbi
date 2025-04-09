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
cargo prototyper [OPTIONS]
```

This builds the firmware based on the provided options (or defaults if none are specified). The resulting files—such as `.elf` executables and `.bin` binaries—are generated in the `target/riscv64imac-unknown-none-elf/release/` directory under your project root. See the "Firmware Compilation" section for specific outputs and modes.

These are necessary for building the firmware and handling RISC-V binary outputs.

#### Options

- `-f, --features <FEATURES>`  
  Enable specific features during the build (supports multiple values, e.g., `--features "feat1,feat2"`).
- `--fdt <PATH>`  
  Specify the path to a Flattened Device Tree (FDT) file.  
  [Environment Variable: `PROTOTYPER_FDT_PATH`]
- `--payload <PATH>`  
  Specify the path to the payload ELF file.  
  [Environment Variable: `PROTOTYPER_PAYLOAD_PATH`]
- `--jump`  
  Enable jump mode.
- `-c, --config-file <PATH>`  
  Specify the path to a custom configuration file.
- `-v, --verbose`  
  Increase logging verbosity (more detailed output).
- `-q, --quiet`  
  Decrease logging verbosity (less output).
- `-h, --help`  
  Display help information.

> #### Note on FDT Files
> 
> Regardless of the mode (Dynamic Firmware, Payload Firmware, or Jump Firmware), specifying an FDT file with `--fdt` ensures it is used to initialize the hardware platform configuration. The FDT file provides essential hardware setup details and overrides the bootloader’s default settings.

### Firmware Compilation

#### 1. Dynamic Firmware

**Compilation Command:**  
Use this command to compile firmware that dynamically loads payloads:

```bash
cargo prototyper
```

**Output:**  
Once compiled, the firmware files will be located in the `target/riscv64imac-unknown-none-elf/release/` directory under your project root:  
- `rustsbi-prototyper-dynamic.elf` (ELF executable)  
- `rustsbi-prototyper-dynamic.bin` (Binary file)

#### 2. Payload Firmware

**Compilation Command:**  
Build firmware with an embedded payload:

```bash
cargo prototyper --payload <PAYLOAD_PATH>
```

**Output:**  
After compilation, the resulting firmware files are generated in the `target/riscv64imac-unknown-none-elf/release/` directory:  
- `rustsbi-prototyper-payload.elf`  
- `rustsbi-prototyper-payload.bin`

#### 3. Jump Firmware

**Compilation Command:**  
Build firmware for jump mode:

```bash
cargo prototyper --jump
```

**Output:**  
After compilation, the resulting firmware files are generated in the `target/riscv64imac-unknown-none-elf/release/` directory:  
- `rustsbi-prototyper-jump.elf`  
- `rustsbi-prototyper-jump.bin`

### Configuration File

Customize bootloader parameters by editing `default.toml` located at `prototyper/config/default.toml`. Example:

```toml
num_hart_max = 8
stack_size_per_hart = 16384  # 16 KiB (16 * 1024)
heap_size = 32768            # 32 KiB (32 * 1024)
page_size = 4096             # 4 KiB
log_level = "INFO"
jump_address = 0x80200000
tlb_flush_limit = 16384      # 16 KiB (page_size * 4)
```

#### Configuration Options

- `num_hart_max`: Maximum number of supported harts (hardware threads).
- `stack_size_per_hart`: Stack size per hart, in bytes.
- `heap_size`: Heap size, in bytes.
- `page_size`: Page size, in bytes.
- `log_level`: Logging level (`TRACE`, `DEBUG`, `INFO`, `WARN`, `ERROR`).
- `jump_address`: Target address for jump mode.
- `tlb_flush_limit`: TLB flush limit, in bytes.

To use a custom configuration file, specify it with:

```bash
cargo prototyper -c /path/to/custom_config.toml
```

### Running an Example

Run the generated firmware in QEMU:

```bash
qemu-system-riscv64 \
  -machine virt \
  -bios target/riscv64imac-unknown-none-elf/release/rustsbi-prototyper-dynamic.elf \
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
