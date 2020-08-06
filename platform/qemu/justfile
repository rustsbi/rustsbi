target := "riscv64gc-unknown-none-elf"
mode := "debug"
build-path := "../../target/" + target + "/" + mode + "/"
m-firmware-file := build-path + "rustsbi-qemu"
m-bin-file := build-path + "rustsbi-qemu.bin"

objcopy := "rust-objcopy --binary-architecture=riscv64"

threads := "1"

build: firmware
    @{{objcopy}} {{m-firmware-file}} --strip-all -O binary {{m-bin-file}}

firmware:
    @cargo build --target={{target}}

qemu: build
    @qemu-system-riscv64 \
            -machine virt \
            -nographic \
            -bios none \
            -device loader,file={{m-bin-file}},addr=0x80000000 \
            -smp threads={{threads}}

run: build qemu
