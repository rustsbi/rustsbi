#!/usr/bin/env bash
# RFENCE acceptance: missing firmware feature, missing CPU H, and strict H/SMP.
# Run from any directory; optional first argument selects the console-log directory.
set -euo pipefail
cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../../.."

rfence_target_dir=$(realpath -m "${CARGO_TARGET_DIR:-target}")
rfence_log_dir=${1:-"$rfence_target_dir/rfence-qemu-logs"}
mkdir -p "$rfence_log_dir"
rfence_kernel="$rfence_target_dir/riscv64imac-unknown-none-elf/release/rustsbi-test-kernel"
rfence_firmware_dir="$rfence_target_dir/riscv64gc-unknown-none-elf/release"

boot_and_check() {
    local label=$1 cpu=$2 bios=$3
    shift 3
    local log_file="$rfence_log_dir/$label.log"
    timeout 60s qemu-system-riscv64 -machine virt -cpu "$cpu" -m 256M \
        -smp 2 -nographic -bios "$bios" >"$log_file" 2>&1 || {
        tail -80 "$log_file"
        return 1
    }
    local pattern
    while IFS= read -r pattern || [[ -n "$pattern" ]]; do
        [[ -z "$pattern" || "$pattern" == \#* ]] && continue
        pattern=${pattern//\{smp\}/2}
        if ! grep -Fq -- "$pattern" "$log_file"; then
            echo "$label: missing expected output: $pattern" >&2
            tail -80 "$log_file"
            return 1
        fi
    done < firmware/test-kernel/scripts/expected.txt
    while IFS= read -r pattern || [[ -n "$pattern" ]]; do
        [[ -z "$pattern" || "$pattern" == \#* ]] && continue
        if grep -Fq -- "$pattern" "$log_file"; then
            echo "$label: forbidden output: $pattern" >&2
            tail -80 "$log_file"
            return 1
        fi
    done < firmware/scripts/qemu-forbidden.txt
    for pattern in '[rfence] concurrent callers complete: 48 rounds per hart' "$@"; do
        grep -F -- "$pattern" "$log_file" || {
            echo "$label: missing RFENCE acceptance marker: $pattern" >&2
            tail -80 "$log_file"
            return 1
        }
    done
    echo "$label: PASS ($log_file)"
}

# A generic kernel exercises the two independent unsupported cases.
cargo prototyper test --no-run
boot_and_check feature-disabled rv64,h=true \
    "$rfence_firmware_dir/rustsbi-prototyper-payload-test.elf" \
    '[rfence] H unavailable returns NOT_SUPPORTED' \
    '[rfence] remote ordinary fence receipt counters pass' \
    '[trap] HS/VS routing pass'
cargo prototyper build --features hypervisor payload "$rfence_kernel.bin"
boot_and_check cpu-without-h rv64,h=false \
    "$rfence_firmware_dir/rustsbi-prototyper-payload.elf" \
    '[rfence] H unavailable returns NOT_SUPPORTED' \
    '[rfence] remote ordinary fence receipt counters pass' \
    '[trap] H extension unavailable'

# Require success for all four HFENCE calls, then check actual guest loads
# after VS/G-stage PTE changes and preservation of a different remote VMID.
CARGO_ENCODED_RUSTFLAGS=$'-C\x1frelocation-model=pie\x1f-C\x1flink-arg=-pie' \
    cargo build --locked -p rustsbi-test-kernel --features hypervisor \
    --target riscv64imac-unknown-none-elf -Z build-std=core --release
rfence_sysroot=$(rustc --print sysroot)
rfence_host=$(rustc -vV | sed -n 's/^host: //p')
"$rfence_sysroot/lib/rustlib/$rfence_host/bin/llvm-objcopy" \
    -O binary "$rfence_kernel" "$rfence_kernel.bin"
cargo prototyper build --features hypervisor payload "$rfence_kernel.bin"
boot_and_check hypervisor-smp rv64,h=true \
    "$rfence_firmware_dir/rustsbi-prototyper-payload.elf" \
    '[rfence] remote ordinary fence receipt counters pass' \
    '[trap] HS/VS routing pass' \
    '[rfence] H local ranges and translation visibility pass' \
    '[rfence] H remote Bare VMID selection pass' \
    '[rfence] H remote delivery, guest restoration and translation visibility pass'
