# Available arguments:
# * General options:
#     - `ARCH`: Target architecture: x86_64, riscv64, aarch64
#     - `PLATFORM`: Target platform in the `platforms` directory
#     - `SMP`: Number of CPUs
#     - `LOG:` Logging level: warn, error, info, debug, trace
#     - `EXTRA_CONFIG`: Extra config specification file
#     - `OUT_CONFIG`: Final config file that takes effect
# * QEMU options:
#     - `DISK`: Path to the virtual disk image
#     - `SBI`: Path to the SBI payload binary

# General options
ARCH ?= riscv64
PLATFORM ?=
SMP ?= 1
LOG ?= debug

OUT_CONFIG ?= $(PWD)/.axconfig.toml
EXTRA_CONFIG ?=

# QEMU options
DISK:= disk.img
SBI ?= rustsbi/target/riscv64gc-unknown-none-elf/release/rustsbi-prototyper-payload.elf
RAMDISK_CPIO:=ramdisk.cpio

export AX_CONFIG_PATH=$(OUT_CONFIG)
export AX_LOG=$(LOG)

include $(PWD)/scripts/make/build.mk
include $(PWD)/scripts/make/utils.mk
include $(PWD)/scripts/make/platform.mk
include $(PWD)/scripts/config/config.mk

defconfig: _axconfig-gen
	$(call defconfig)

oldconfig: _axconfig-gen
	$(call oldconfig)

clean:
	cargo clean
	test -d "rustsbi" && cd rustsbi && cargo clean
	rm -f $(OUT_CONFIG)

build: clean defconfig all

qemu-run:
	qemu-system-riscv64 -m 128M -serial mon:stdio -bios $(SBI) -nographic -machine virt -nographic -machine virt -device virtio-blk-pci,drive=disk0 -drive id=disk0,if=none,format=raw,file=$(DISK)

qemu-display:
	qemu-system-riscv64 -m 128M -serial mon:stdio -bios $(SBI) -machine virt -device virtio-blk-pci,drive=disk0 -drive id=disk0,if=none,format=raw,file=$(DISK) -device virtio-gpu-pci,xres=1024,yres=768
