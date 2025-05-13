BOOT:= arceboot.bin
DISK:= fat32_disk_test.img

OUT_CONFIG ?= $(PWD)/.axconfig.toml

export AX_CONFIG_PATH=$(OUT_CONFIG)

clean:
	cargo clean
	rm arceboot.bin

build:
	./build.sh

run:
	qemu-system-riscv64 -serial mon:stdio -bios rustsbi-prototyper.bin -nographic -machine virt -kernel $(BOOT) -device virtio-blk-pci,drive=disk0 -drive id=disk0,if=none,format=raw,file=$(DISK)