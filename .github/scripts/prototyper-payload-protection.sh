#!/usr/bin/env bash
# STA SET_SHMEM must not modify the embedded payload storage.
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/../.."
target_dir=${CARGO_TARGET_DIR:-target}
work_dir="$target_dir/payload-protection"
log_dir=qemu-logs
qemu=${QEMU_RISCV64:-qemu-system-riscv64}
mkdir -p "$work_dir" "$log_dir"

linker="$work_dir/probe.ld"
cat >"$linker" <<'EOF'
OUTPUT_ARCH(riscv)
ENTRY(_start)
SECTIONS {
  . = 0x80200000;
  .text : { *(.text.entry) . = ALIGN(4); *(.text.trap) *(.text .text.*) }
  .rodata : { *(.rodata .rodata.*) }
  .data : { *(.data .data.*) *(.sdata .sdata.*) }
  .bss : ALIGN(4096) { *(.bss .bss.*) *(.sbss .sbss.*) }
  /DISCARD/ : { *(.eh_frame) }
}
EOF

guest="$work_dir/probe.elf"
binary="$work_dir/probe.bin"
rustc --edition=2024 --target=riscv64imac-unknown-none-elf -Cpanic=abort \
  -Copt-level=2 "-Clink-arg=-T$linker" -Clink-arg=--no-relax \
  .github/scripts/payload-protection.rs -o "$guest"
rust-objcopy -O binary "$guest" "$binary"

cargo prototyper build payload "$binary"

bios="$target_dir/riscv64gc-unknown-none-elf/release/rustsbi-prototyper-payload.elf"
log_file="$log_dir/payload-protection.log"
if timeout 30s "$qemu" -machine virt -m 256M -smp 1 -nographic \
  -bios "$bios" >"$log_file" 2>&1; then
  qemu_exit=0
else
  qemu_exit=$?
fi
if [[ "$qemu_exit" != 0 ]] || ! grep -Fq 'Payload protection: PASS' "$log_file"; then
  echo "[payload-protection] QEMU exit: $qemu_exit; log: $log_file" >&2
  cat "$log_file" >&2
  exit 1
fi
echo "payload-protection: PASS (SET_SHMEM cannot modify the embedded payload)"
