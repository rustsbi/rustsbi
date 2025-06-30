# Available arguments:
# * General options:
#     - `ARCH`: Target architecture: x86_64, riscv64, aarch64
#     - `PLATFORM`: Target platform in the `platforms` directory
#     - `SMP`: Number of CPUs
#     - `LOG:` Logging level: warn, error, info, debug, trace
#     - `MEDIUM:` Boot Medium Type: ramdisk-cpio, virtio-blk
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

# 下面的目前还没用, 现在需要手动去cargo.toml中修改, 后面补上
MEDIUM ?= ramdisk-cpio

OUT_CONFIG ?= $(PWD)/.axconfig.toml
EXTRA_CONFIG ?=

# QEMU options
DISK:= disk.img
SBI:=rustsbi/target/riscv64imac-unknown-none-elf/release/rustsbi-prototyper-payload.elf
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
	cd rustsbi && cargo clean
	rm -f $(OUT_CONFIG)

build: clean defconfig all

ramdiskcpio:
	qemu-system-riscv64 -m 128M -serial mon:stdio -bios $(SBI) -nographic -machine virt -device loader,file=$(RAMDISK_CPIO),addr=0x84000000

virtiodisk:
	qemu-system-riscv64 -m 128M -serial mon:stdio -bios $(SBI) -nographic -machine virt -device virtio-blk-pci,drive=disk0 -drive id=disk0,if=none,format=raw,file=$(DISK)
