#!/usr/bin/env bash
# Usage: bash firmware/test-kernel/scripts/test-machine-stack.sh [32|64]
set -euo pipefail

repo_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../../.." && pwd)
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
artifact_dir="$CARGO_TARGET_DIR/machine-stack/rv$xlen"
log_dir=${QEMU_LOG_DIR:-"$repo_dir/qemu-logs"}
mkdir -p "$artifact_dir" "$log_dir"

cat > "$artifact_dir/config.toml" <<'TOML'
heap_size = 0x15000
page_size = 4096
log_level = "INFO"
link_start_address = 0x80400000
payload_address = 0x84000000
jump_address = 0x84000000
tlb_flush_limit = 16384

[[next_addr]]
start = 0x80000000
end = 0x90000000
TOML

for image in loader payload; do
    case "$image" in
        loader) address=0x80000000 ;;
        payload) address=0x84000000 ;;
    esac
    cat > "$artifact_dir/$image.ld" <<EOF
OUTPUT_ARCH(riscv)
ENTRY(_start)
SECTIONS {
    . = $address;
    .text : { *(.text.entry) *(.text .text.*) }
    .rodata : { *(.rodata .rodata.*) }
}
EOF
    rustc --edition=2024 --target "$target" -C panic=abort -C opt-level=2 \
        -C link-arg="-T$artifact_dir/$image.ld" -o "$artifact_dir/$image.elf" \
        "firmware/test-kernel/fixtures/machine-stack/$image.rs"
done
rust-objcopy -O binary "$artifact_dir/loader.elf" "$artifact_dir/loader.bin"
cargo prototyper build --target "$target" --config-file "$artifact_dir/config.toml" dynamic

qemu=${QEMU:-qemu-system-riscv$xlen}
log_file="$log_dir/machine-stack-rv$xlen.log"
if ! timeout 30s "$qemu" -machine virt -m 256M -smp 1 -nographic \
    -bios "$artifact_dir/loader.bin" \
    -device "loader,file=$CARGO_TARGET_DIR/$target/release/rustsbi-prototyper-dynamic.elf" \
    -device "loader,file=$artifact_dir/payload.elf" > "$log_file" 2>&1; then
    cat "$log_file"
    exit 1
fi
grep -F '[machine-stack] two interrupt returns pass' "$log_file"
