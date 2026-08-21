#!/usr/bin/env bash
set -euo pipefail

mode=${1:?missing mode}
kernel=${2:?missing kernel}
log_dir=${3:-qemu-logs}

mkdir -p "$log_dir"

case "$kernel" in
  test)
    smp=1
    attempts=${QEMU_BOOT_TEST_RETRIES:-2}
    timeout_secs=${QEMU_BOOT_TEST_TIMEOUT_SECS:-60}
    payload_bin="target/riscv64imac-unknown-none-elf/release/rustsbi-test-kernel.bin"
    payload_elf="target/riscv64gc-unknown-none-elf/release/rustsbi-prototyper-payload-test.elf"
    ;;
  bench)
    smp=4
    attempts=${QEMU_BOOT_BENCH_RETRIES:-4}
    timeout_secs=${QEMU_BOOT_BENCH_TIMEOUT_SECS:-90}
    payload_bin="target/riscv64imac-unknown-none-elf/release/rustsbi-bench-kernel.bin"
    payload_elf="target/riscv64gc-unknown-none-elf/release/rustsbi-prototyper-payload-bench.elf"
    ;;
  *)
    echo "unknown kernel: $kernel" >&2
    exit 1
    ;;
esac

case "$mode" in
  payload)
    bios="$payload_elf"
    extra_args=()
    ;;
  dynamic)
    bios="target/riscv64gc-unknown-none-elf/release/rustsbi-prototyper-dynamic.elf"
    extra_args=(-kernel "$payload_bin")
    ;;
  jump)
    bios="target/riscv64gc-unknown-none-elf/release/rustsbi-prototyper-jump.elf"
    extra_args=(-device "loader,file=$payload_bin,addr=0x80200000")
    ;;
  *)
    echo "unknown mode: $mode" >&2
    exit 1
    ;;
esac

log_file="$log_dir/prototyper-${mode}-${kernel}.log"

run_once() {
  local attempt=$1

  set +e
  timeout "${timeout_secs}s" qemu-system-riscv64 \
    -machine virt \
    -m 256M \
    -smp "$smp" \
    -nographic \
    -bios "$bios" \
    "${extra_args[@]}" \
    >"$log_file" 2>&1
  qemu_exit=$?
  set -e

  echo "[$mode/$kernel] attempt $attempt/$attempts qemu exit: $qemu_exit (timeout=${timeout_secs}s)"
  test "$qemu_exit" = "0" || return 1
  test -s "$log_file" || return 1

  grep -F 'Hello RustSBI!' "$log_file" || return 1
  grep -F "Platform HART Count           : $smp" "$log_file" || return 1

  # Boot-policy order guard: the boot-hart presentation sequence must
  # appear in phase order. A reorder or drop means the boot policy
  # changed even when every substring grep stays green.
  awk 'BEGIN {
         n = split("Boot HART ID|Boot HART Privileged Version:|Boot HART MHPM Mask:|Redirecting hart", want, "|")
       }
       /Boot HART ID|Boot HART Privileged Version:|Boot HART MHPM Mask:|Redirecting hart/ {
         count++
         if ($0 !~ want[count]) fail = 1
       }
       END { exit !(count == n && !fail) }' "$log_file" || return 1

  case "$kernel" in
    test)
      grep -F 'Sbi `Base` test pass' "$log_file" || return 1
      grep -F 'Sbi `TIME` test pass' "$log_file" || return 1
      grep -F 'Sbi `sPI` test pass' "$log_file" || return 1
      grep -F 'Sbi `DBCN` test pass' "$log_file" || return 1
      grep -F 'DBCN rejected non-zero upper-half write' "$log_file" || return 1
      grep -F 'DBCN rejected non-zero upper-half read' "$log_file" || return 1
      grep -F '[pmu] counters number:' "$log_file" || return 1
      ;;
    bench)
      grep -F 'Starting test' "$log_file" || return 1
      grep -F 'Test #0:' "$log_file" || return 1
      grep -F 'Test #1:' "$log_file" || return 1
      grep -F 'Test #2:' "$log_file" || return 1
      grep -F 'Test #3:' "$log_file" || return 1
      ;;
  esac

  ! grep -En 'panicked|FAILED|SystemFailure' "$log_file" || return 1
}

for attempt in $(seq 1 "$attempts"); do
  if run_once "$attempt"; then
    echo "[$mode/$kernel] log: $log_file"
    exit 0
  fi

  if [ "${qemu_exit:-1}" != "124" ]; then
    break
  fi

  if [ "$attempt" -lt "$attempts" ]; then
    echo "[$mode/$kernel] retrying after attempt $attempt"
  fi
done

echo "[$mode/$kernel] final log tail"
tail -n 120 "$log_file" || true
exit 1
