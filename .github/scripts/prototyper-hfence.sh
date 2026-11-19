#!/usr/bin/env bash
set -euo pipefail

# Regression for issue #498: on a platform where one hart reports no
# hypervisor extension, every HFENCE call targeting it must return
# NOT_SUPPORTED before anything executes locally, and hfence.gvma must
# encode page addresses as `GPA >> 2`.
#
# The DTB handed to the firmware, not the QEMU CPU model, establishes the
# mixed-H feature records: QEMU still implements the H instructions, so the
# pre-fix firmware executed them and returned success. This test does not
# claim to cover illegal-instruction behavior on real hardware without H.

target_dir=${CARGO_TARGET_DIR:-target}
work_dir="$target_dir/hfence-regression"
log_file="$work_dir/qemu-mixed-h.log"
forbidden_file="firmware/scripts/qemu-forbidden.txt"

pass_line="Sbi mixed-H HFENCE preflight test pass"
done_line="SBI tests completed: PASS"

mkdir -p "$work_dir"

# Build a two-hart DTB with QEMU and hide the hypervisor extension from both
# harts' software-visible records. The firmware keeps the boot hart's record
# through its hardware `misa` probe, and payload-mode QEMU elects the boot
# hart by a lock race, so hiding `h` from every DTB hart is the only
# construction where exactly one hart (the non-boot one) reports no H
# regardless of the election outcome. The full extension list is read back
# instead of hardcoding an ISA string, so the script does not depend on the
# QEMU version's extension set.
mixed_dtb="$work_dir/mixed-h.dtb"
qemu-system-riscv64 \
  -machine "virt,dumpdtb=$mixed_dtb" \
  -m 256M -smp 2 -cpu rv64,h=true -display none

for cpu in cpu@0 cpu@1; do
  isa_list=$(fdtget "$mixed_dtb" "/cpus/$cpu" riscv,isa-extensions) || {
    echo "failed to read riscv,isa-extensions from /cpus/$cpu" >&2
    exit 1
  }
  read -r -a isa_extensions <<< "$isa_list"

  cpu_extensions=()
  for extension in "${isa_extensions[@]}"; do
    # Only drop the exact `h` entry; keep every other extension.
    [ "$extension" = "h" ] || cpu_extensions+=("$extension")
  done
  fdtput -t s "$mixed_dtb" "/cpus/$cpu" riscv,isa-extensions "${cpu_extensions[@]}"
done

list_contains() {
  local item
  for item in $1; do
    [ "$item" = "$2" ] && return 0
  done
  return 1
}

cpu0_isa=$(fdtget "$mixed_dtb" /cpus/cpu@0 riscv,isa-extensions)
cpu1_isa=$(fdtget "$mixed_dtb" /cpus/cpu@1 riscv,isa-extensions)
if list_contains "$cpu0_isa" h || list_contains "$cpu1_isa" h; then
  echo "no DTB hart may report the hypervisor extension anymore" >&2
  exit 1
fi

# The regression is gated by `option_env!("HFENCE_MIXED_H")`; cargo tracks
# that variable and rebuilds the kernel when it changes. If a build ever
# reused a stale kernel without the regression, the run would fail loudly on
# the missing pass line below — it cannot pass silently.
kernel_target=riscv64imac-unknown-none-elf
kernel_bin="$target_dir/$kernel_target/release/rustsbi-test-kernel.bin"
HFENCE_MIXED_H=1 \
CARGO_ENCODED_RUSTFLAGS=$'-C\037relocation-model=pie\037-C\037link-arg=-pie' \
  cargo build --locked -p rustsbi-test-kernel \
    --target "$kernel_target" --release -Z build-std=core

rust-objcopy -O binary \
  "$target_dir/$kernel_target/release/rustsbi-test-kernel" \
  "$kernel_bin"

# Firmware with the hypervisor feature, embedding the mixed-H DTB and the
# mixed-H test kernel.
cargo prototyper build --features hypervisor \
  --fdt "$mixed_dtb" payload "$kernel_bin"
payload_elf="$target_dir/riscv64gc-unknown-none-elf/release/rustsbi-prototyper-payload.elf"
test -s "$payload_elf"

# Boot with two harts. Hardware still implements H; the DTB decides what the
# firmware publishes about each hart.
set +e
timeout 60s qemu-system-riscv64 \
  -machine virt \
  -m 256M \
  -smp 2 \
  -cpu rv64,h=true \
  -nographic \
  -bios "$payload_elf" \
  >"$log_file" 2>&1
qemu_exit=$?
set -e

report_failure() {
  echo "mixed-H HFENCE regression failed (qemu exit: $qemu_exit)" >&2
  echo "--- full QEMU log ($log_file) ---" >&2
  cat "$log_file" >&2
  exit 1
}

[ "$qemu_exit" = "0" ] || report_failure
[ -s "$log_file" ] || report_failure

grep -Fq "$pass_line" "$log_file" || {
  echo "missing expected line: $pass_line" >&2
  report_failure
}
grep -Fq "$done_line" "$log_file" || {
  echo "missing expected line: $done_line" >&2
  report_failure
}

# Forbidden patterns follow the shared firmware list. Fail closed: a
# missing or empty file must not silently disable the check.
test -s "$forbidden_file" || {
  echo "missing or empty forbidden pattern file: $forbidden_file" >&2
  exit 1
}
forbidden_count=0
while IFS= read -r pattern || [ -n "$pattern" ]; do
  pattern=${pattern%%"${pattern##*[![:space:]]}"}
  pattern=${pattern#"${pattern%%[![:space:]]*}"}
  case "$pattern" in ''|'#'*) continue ;; esac
  ((forbidden_count += 1))
  if grep -Fq "$pattern" "$log_file"; then
    echo "forbidden pattern present: $pattern" >&2
    report_failure
  fi
done < "$forbidden_file"
test "$forbidden_count" -gt 0 || {
  echo "no patterns in: $forbidden_file" >&2
  exit 1
}

echo "mixed-H HFENCE regression pass; log: $log_file"
