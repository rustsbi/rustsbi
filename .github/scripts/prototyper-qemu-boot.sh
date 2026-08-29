#!/usr/bin/env bash
set -euo pipefail

mode=${1:?missing mode}
kernel=${2:?missing kernel}
log_dir=${3:-qemu-logs}

# Single source for the verification patterns, shared with xtask
# (`Kernel::expected_patterns` / `kernels::forbidden_patterns`).
expected_file="firmware/${kernel}-kernel/scripts/expected.txt"
forbidden_file="firmware/scripts/qemu-forbidden.txt"

mkdir -p "$log_dir"

case "$kernel" in
  test)
    smp=1
    attempts=${QEMU_BOOT_TEST_RETRIES:-2}
    timeout_secs=${QEMU_BOOT_TEST_TIMEOUT_SECS:-60}
    payload_bin="target/riscv64imac-unknown-none-elf/release/rustsbi-test-kernel.bin"
    ;;
  bench)
    smp=4
    attempts=${QEMU_BOOT_BENCH_RETRIES:-4}
    timeout_secs=${QEMU_BOOT_BENCH_TIMEOUT_SECS:-90}
    payload_bin="target/riscv64imac-unknown-none-elf/release/rustsbi-bench-kernel.bin"
    ;;
  *)
    echo "unknown kernel: $kernel" >&2
    exit 1
    ;;
esac

# Payload-mode boots are verified by `cargo prototyper test` / `bench`
# themselves; this script covers the dynamic and jump firmware modes.
case "$mode" in
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

  # Fail closed: a missing or empty pattern file must not silently
  # disable console verification (the process-substitution loops below
  # would otherwise run zero iterations and still succeed).
  test -s "$expected_file" || {
    echo "[$mode/$kernel] missing or empty pattern file: $expected_file" >&2
    return 1
  }
  test -s "$forbidden_file" || {
    echo "[$mode/$kernel] missing or empty pattern file: $forbidden_file" >&2
    return 1
  }

  # Patterns are read with the same semantics as the xtask side
  # (`read_console_patterns`): trimmed lines, `#` comments skipped, and
  # a missing trailing newline still yields the last line.
  while IFS= read -r pattern || [ -n "$pattern" ]; do
    pattern=${pattern%%"${pattern##*[![:space:]]}"}
    pattern=${pattern#"${pattern%%[![:space:]]*}"}
    case "$pattern" in ''|'#'*) continue ;; esac
    grep -Fq "$pattern" "$log_file" || return 1
  done < <(sed "s/{smp}/$smp/g" "$expected_file")

  # Dispatcher-backed extension wiring: these lines render from the
  # published SBI_DISPATCHER (presence chains + Once publish). A missing
  # line means an extension was silently dropped during boot assembly.
  grep -F 'Platform HSM Extension        : Available' "$log_file" || return 1
  grep -F 'Platform RFence Extension     : Available' "$log_file" || return 1
  grep -F 'Platform SUSP Extension       : Available' "$log_file" || return 1
  grep -F 'Platform PMU Extension        : Available' "$log_file" || return 1

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

  while IFS= read -r pattern || [ -n "$pattern" ]; do
    pattern=${pattern%%"${pattern##*[![:space:]]}"}
    pattern=${pattern#"${pattern%%[![:space:]]*}"}
    case "$pattern" in ''|'#'*) continue ;; esac
    if grep -Fq "$pattern" "$log_file"; then
      return 1
    fi
  done < "$forbidden_file"
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
