target := "riscv64imac-unknown-none-elf"
mode := "debug"
build-path := "../../target/" + target + "/" + mode + "/"
m-firmware-file := build-path + "rustsbi-k210"
m-bin-file := build-path + "rustsbi-k210.bin"

k210-serialport := "/dev/ttyS17"

objdump := "rust-objdump"
objcopy := "rust-objcopy --binary-architecture=riscv64"

build: firmware
    @{{objcopy}} {{m-firmware-file}} --strip-all -O binary {{m-bin-file}}

firmware:
    @cargo build --target={{target}}

run: build
    @python3 ./kflash.py --port {{k210-serialport}} --terminal {{m-bin-file}}

asm: build
    @{{objdump}} -D {{m-firmware-file}} | less
