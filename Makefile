DISK:= fat32_disk_test.img
SBI:=rustsbi/target/riscv64imac-unknown-none-elf/release/rustsbi-prototyper-payload.elf

OUT_CONFIG ?= $(PWD)/.axconfig.toml

export AX_CONFIG_PATH=$(OUT_CONFIG)

clean:
	cargo clean
	cd rustsbi && cargo clean

build: clean
	./build.sh

run:
	qemu-system-riscv64 -serial mon:stdio -bios $(SBI) -nographic -machine virt -device virtio-blk-pci,drive=disk0 -drive id=disk0,if=none,format=raw,file=$(DISK)