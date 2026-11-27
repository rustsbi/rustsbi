#!/usr/bin/env bash
# Usage: bash .github/scripts/prototyper-relocated-fault.sh [32|64]
#
# The firmware is linked at 0x80000000 and loaded at 0x80200000, then faults
# before Runtime installs its own trap vector. The trap must reach the
# relocated fail-stop vector and park the hart; a link-time vector instead
# points at unloaded memory and traps forever.
set -euo pipefail

repo_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)
cd "$repo_dir"
xlen=${1:-64}
case "$xlen" in
    32) target=riscv32imac-unknown-none-elf ;;
    64) target=riscv64imac-unknown-none-elf ;;
    *) echo "expected XLEN 32 or 64, got: $xlen" >&2; exit 2 ;;
esac

export CARGO_TARGET_DIR=${CARGO_TARGET_DIR:-"$repo_dir/target"}
case "$CARGO_TARGET_DIR" in
    /*) ;;
    *) export CARGO_TARGET_DIR="$repo_dir/$CARGO_TARGET_DIR" ;;
esac
artifact_dir="$CARGO_TARGET_DIR/relocated-fault/rv$xlen"
log_dir=${QEMU_LOG_DIR:-"$repo_dir/qemu-logs"}
mkdir -p "$artifact_dir" "$log_dir"

cat > "$artifact_dir/loader.ld" <<'EOF'
OUTPUT_ARCH(riscv)
ENTRY(_start)
SECTIONS {
    . = 0x80000000;
    .text : { *(.text.entry) *(.text .text.*) }
    .rodata : { *(.rodata .rodata.*) }
}
EOF

rustc --edition=2024 --target "$target" -C panic=abort -C opt-level=2 \
    -C link-arg="-T$artifact_dir/loader.ld" -o "$artifact_dir/loader.elf" \
    .github/scripts/relocated-fault/loader.rs
rust-objcopy -O binary "$artifact_dir/loader.elf" "$artifact_dir/loader.bin"
cargo prototyper build --target "$target" dynamic

qemu=${QEMU:-qemu-system-riscv$xlen}
int_log="$log_dir/relocated-fault-rv$xlen.log"
# fail_stop parks the hart, so the timeout is the expected outcome.
timeout 10s "$qemu" -machine virt -m 256M -smp 1 -nographic \
    -bios "$artifact_dir/loader.bin" \
    -device "loader,file=$CARGO_TARGET_DIR/$target/release/rustsbi-prototyper-dynamic.bin,addr=0x80200000" \
    -d int -D "$int_log" </dev/null >/dev/null 2>&1 || true

faults=$(grep -c 'riscv_cpu_do_interrupt' "$int_log" || true)
if [[ "$faults" -ne 1 ]] || ! grep -q 'fault_load' "$int_log" || ! grep -q 'deadb00' "$int_log"; then
    echo "expected one load fault into the relocated fail-stop vector, got $faults" >&2
    head -20 "$int_log" >&2
    exit 1
fi
echo "relocated fail-stop vector: PASS"
