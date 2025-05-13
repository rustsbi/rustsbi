#!/bin/bash
set -e

TARGET=riscv64gc-unknown-none-elf

ELF=target/$TARGET/release/arceboot

# 编译
cargo rustc --release --target $TARGET -- \
    -C opt-level=z \
    -C panic=abort \
    -C relocation-model=static \
    -C target-cpu=generic \
    -C link-args="-Tlink.ld"

rust-objcopy --binary-architecture=riscv64 $ELF --strip-all -O binary arceboot.bin

echo "BIOS binary generated: arceboot.bin"