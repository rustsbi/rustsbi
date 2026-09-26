#!/usr/bin/env bash
# Boot the default Prototyper test image as Miralis-managed M-mode firmware.
set -euo pipefail

miralis=${MIRALIS_IMAGE:-miralis/target/riscv-unknown-miralis/debug/miralis.img}
firmware=target/riscv64gc-unknown-none-elf/release/rustsbi-prototyper-payload-test.bin
expected=firmware/test-kernel/scripts/expected.txt
forbidden=firmware/scripts/qemu-forbidden.txt
log=qemu-logs/prototyper-miralis-test.log
trace=qemu-logs/prototyper-miralis-traps.log
timeout_secs=${MIRALIS_BOOT_TIMEOUT_SECS:-120}

for image in "$miralis" "$firmware" "$expected" "$forbidden"; do
  test -s "$image" || { echo "missing or empty input: $image" >&2; exit 1; }
done
mkdir -p qemu-logs

set +e
timeout "${timeout_secs}s" qemu-system-riscv64 \
  -machine virt -m 256M -smp 1 -nographic -no-reboot -bios "$miralis" \
  -device "loader,file=$firmware,addr=0x80200000,force-raw=on" \
  -d int,guest_errors -D "$trace" \
  >"$log" 2>&1
status=$?
set -e

if (( status != 0 )); then
  echo "QEMU exited with status $status; last 120 log lines:" >&2
  tail -n 120 "$log" >&2
  echo "Last trapped instructions:" >&2
  tail -n 20 "$trace" >&2
  exit 1
fi

while IFS= read -r pattern || [[ -n $pattern ]]; do
  pattern=${pattern//\{smp\}/1}
  [[ -z $pattern || $pattern == \#* ]] && continue
  grep -Fq -- "$pattern" "$log" || {
    echo "missing expected output: $pattern" >&2
    tail -n 120 "$log" >&2
    exit 1
  }
done < "$expected"

while IFS= read -r pattern || [[ -n $pattern ]]; do
  [[ -z $pattern || $pattern == \#* ]] && continue
  if grep -Fq -- "$pattern" "$log"; then
    echo "forbidden output: $pattern" >&2
    tail -n 120 "$log" >&2
    exit 1
  fi
done < "$forbidden"

echo "Miralis → Prototyper → test kernel boot passed; log: $log"
