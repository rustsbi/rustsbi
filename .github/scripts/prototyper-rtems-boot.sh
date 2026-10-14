#!/usr/bin/env bash
#
# Boot RTEMS in S-mode (RISCV_USE_S_MODE=True) through RustSBI Prototyper in QEMU.
# Target is qemu-virt64 with rv64imafdc BSP and Sstc timer extension.
# RustSBI operates as QEMU's M-mode firmware here.
# In S-mode with Sstc (rv64,sstc=true), RTEMS clock driver directly programs stimecmp.
# Official BSP_RESET_BOARD_AT_EXIT=0 leaves the guest idle after clean shutdown.

set -euo pipefail

readonly RTEMS_RELEASE="rtems7-ticker-s-mode-v1"
readonly RTEMS_IMAGE="rtems7-riscv64-s-mode-ticker.exe"
readonly RTEMS_IMAGE_SHA256="a201e6574d25a3db81a0ca346327f4e901c3f5891e1398e3fdb8a66b3a32d25b"
readonly RTEMS_IMAGE_URL="https://github.com/rustsbi/rtems/releases/download/${RTEMS_RELEASE}/${RTEMS_IMAGE}"

readonly RUSTSBI_MARKER='Hello RustSBI!'
readonly TEST_START_MARKER='*** BEGIN OF TEST CLOCK TICK ***'
readonly SUCCESS_MARKER='*** END OF TEST CLOCK TICK ***'

readonly BOOT_FAILURE_PATTERN='panicked|FAILED|FATAL|Fatal exception|Unhandled exception|Kernel panic|not syncing|System shutdown scheduled due to RustSBI panic'

readonly REPO_ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)"

readonly CACHE_DIR="${RTEMS_CACHE_DIR:-${REPO_ROOT}/.cache/rtems/${RTEMS_RELEASE}}"
readonly LOG_DIR="${QEMU_LOG_DIR:-${REPO_ROOT}/qemu-logs}"
readonly LOG_NAME="prototyper-rtems.log"

readonly QEMU="${RTEMS_QEMU:-qemu-system-riscv64}"
readonly CPU="${RTEMS_CPU:-rv64,sstc=true}"
readonly MEMORY_MB="${RTEMS_MEMORY_MB:-256}"
readonly BOOT_TIMEOUT_SECS="${RTEMS_BOOT_TIMEOUT_SECS:-180}"
readonly SETTLE_SECS="${RTEMS_SETTLE_SECS:-5}"
readonly JOBS="${RTEMS_JOBS:-4}"

abspath() {
  if realpath -m "" >/dev/null 2>&1; then
    realpath -m "$1"
  else
    case "$1" in
      /*) printf '%s\n' "$1" ;;
      *) printf '%s\n' "$REPO_ROOT/$1" ;;
    esac
  fi
}

readonly CACHE_DIR_ABS="$(abspath "$CACHE_DIR")"
readonly LOG_DIR_ABS="$(abspath "$LOG_DIR")"
readonly LOG_FILE_ABS="${LOG_DIR_ABS}/${LOG_NAME}"
readonly RUSTSBI_ABS="$(abspath "${RTEMS_RUSTSBI:-${CARGO_TARGET_DIR:-${REPO_ROOT}/target}/riscv64gc-unknown-none-elf/release/rustsbi-prototyper-dynamic.elf}")"

readonly TICKER_ELF_ABS="${CACHE_DIR_ABS}/${RTEMS_IMAGE}"

QEMU_PID=""
TERMINATED_BY_SCRIPT=0
LAST_QEMU_EXIT=0

reap_qemu() {
  local target_pid="${1:-$QEMU_PID}"
  if [[ -n "$target_pid" ]]; then
    local rc=0
    wait "$target_pid" 2>/dev/null || rc=$?
    LAST_QEMU_EXIT=$rc
    QEMU_PID=""
  fi
}

stop_qemu() {
  local target_pid="$QEMU_PID"
  if [[ -n "$target_pid" ]] && kill -0 "$target_pid" 2>/dev/null; then
    if kill "$target_pid" 2>/dev/null; then
      TERMINATED_BY_SCRIPT=1
    fi
    reap_qemu "$target_pid"
  elif [[ -n "$target_pid" ]]; then
    reap_qemu "$target_pid"
  fi
}

on_signal() {
  stop_qemu
  exit 130
}

trap 'on_signal' INT TERM
trap stop_qemu EXIT

verify_image() {
  printf '%s  %s\n' "$RTEMS_IMAGE_SHA256" "$1" | sha256sum --check --status
}

download_image() {
  mkdir -p "$CACHE_DIR_ABS"
  if [[ -f "$TICKER_ELF_ABS" ]] && verify_image "$TICKER_ELF_ABS"; then
    echo "Using verified RTEMS ticker: $TICKER_ELF_ABS"
    return
  fi

  local temp
  temp=$(mktemp "${TICKER_ELF_ABS}.part.XXXXXX")
  echo "Downloading prebuilt RTEMS ticker..."
  if ! curl --fail --location --connect-timeout 15 --max-time 300 \
    --retry 3 --output "$temp" "$RTEMS_IMAGE_URL"; then
    rm -f "$temp"
    echo "Error: Failed to download RTEMS ticker" >&2
    return 1
  fi
  if ! verify_image "$temp"; then
    rm -f "$temp"
    echo "Error: RTEMS ticker checksum mismatch" >&2
    return 1
  fi
  mv -f "$temp" "$TICKER_ELF_ABS"
}

build_rustsbi() {
  echo "Building RustSBI Prototyper (dynamic mode) from current tree..."
  (
    cd "$REPO_ROOT"
    CARGO_BUILD_JOBS="$JOBS" cargo prototyper build dynamic
  )
  test -s "$RUSTSBI_ABS" || {
    echo "Error: Missing $RUSTSBI_ABS" >&2
    return 1
  }
}

run_boot_test() {
  mkdir -p "$LOG_DIR_ABS"
  : > "$LOG_FILE_ABS"

  if ! command -v "$QEMU" >/dev/null 2>&1; then
    echo "Error: QEMU binary '$QEMU' not found or not executable" >&2
    return 1
  fi

  echo "Starting QEMU verification..."
  echo "  QEMU: $QEMU"
  echo "  CPU: $CPU"
  echo "  BIOS (RustSBI): $RUSTSBI_ABS"
  echo "  Kernel (RTEMS): $TICKER_ELF_ABS"
  echo "  Log: $LOG_FILE_ABS"

  "$QEMU" \
    -machine virt \
    -cpu "$CPU" \
    -m "${MEMORY_MB}M" \
    -smp 1 \
    -display none \
    -serial stdio \
    -monitor none \
    -bios "$RUSTSBI_ABS" \
    -kernel "$TICKER_ELF_ABS" \
    </dev/null >"$LOG_FILE_ABS" 2>&1 &
  QEMU_PID=$!

  local start_time
  start_time=$(date +%s)
  TERMINATED_BY_SCRIPT=0
  LAST_QEMU_EXIT=0

  while true; do
    if ! kill -0 "$QEMU_PID" 2>/dev/null; then
      reap_qemu "$QEMU_PID"
      break
    fi

    if [[ -s "$LOG_FILE_ABS" ]]; then
      if grep -E -q "$BOOT_FAILURE_PATTERN" "$LOG_FILE_ABS"; then
        echo "Detected boot failure pattern in log!" >&2
        stop_qemu
        return 1
      fi

      if grep -F -q "$SUCCESS_MARKER" "$LOG_FILE_ABS"; then
        echo "Detected success marker! Settling for ${SETTLE_SECS}s to catch late exits or faults..."
        local settle_end
        settle_end=$(( $(date +%s) + SETTLE_SECS ))
        while (( $(date +%s) < settle_end )); do
          if ! kill -0 "$QEMU_PID" 2>/dev/null; then
            reap_qemu "$QEMU_PID"
            echo "QEMU exited naturally during settle with code: $LAST_QEMU_EXIT"
            break
          fi
          if grep -E -q "$BOOT_FAILURE_PATTERN" "$LOG_FILE_ABS"; then
            echo "Detected late failure pattern during settle!" >&2
            stop_qemu
            return 1
          fi
          sleep 0.5
        done

        if [[ -n "$QEMU_PID" ]]; then
          echo "QEMU still running after settle period; stopping via SIGTERM..."
          stop_qemu
        fi

        break
      fi
    fi

    local current_time
    current_time=$(date +%s)
    if (( current_time - start_time >= BOOT_TIMEOUT_SECS )); then
      # Avoid missing a success marker that arrives at the timeout boundary.
      if [[ -s "$LOG_FILE_ABS" ]] && grep -F -q "$SUCCESS_MARKER" "$LOG_FILE_ABS" && ! grep -E -q "$BOOT_FAILURE_PATTERN" "$LOG_FILE_ABS"; then
        echo "Success marker observed at timeout boundary; stopping QEMU..."
        stop_qemu
        break
      fi
      echo "Timeout waiting for RTEMS ticker boot (${BOOT_TIMEOUT_SECS}s)" >&2
      stop_qemu
      return 1
    fi

    sleep 1
  done

  if [[ -n "$QEMU_PID" ]]; then
    stop_qemu
  fi

  echo "Verifying log results..."
  if grep -E -q "$BOOT_FAILURE_PATTERN" "$LOG_FILE_ABS"; then
    echo "FAILED: Log contains forbidden failure pattern" >&2
    return 1
  fi

  if (( TERMINATED_BY_SCRIPT == 1 )); then
    if [[ $LAST_QEMU_EXIT -ne 0 && $LAST_QEMU_EXIT -ne 143 ]]; then
      echo "Error: QEMU terminated with unexpected exit code $LAST_QEMU_EXIT (expected 0 or 143 on SIGTERM)" >&2
      return 1
    fi
  else
    if [[ $LAST_QEMU_EXIT -ne 0 ]]; then
      echo "Error: QEMU exited naturally with non-zero exit code $LAST_QEMU_EXIT" >&2
      return 1
    fi
  fi

  if ! grep -F -q "$RUSTSBI_MARKER" "$LOG_FILE_ABS"; then
    echo "FAILED: Log missing RustSBI marker: $RUSTSBI_MARKER" >&2
    return 1
  fi

  if ! grep -F -q "$TEST_START_MARKER" "$LOG_FILE_ABS"; then
    echo "FAILED: Log missing ticker start marker: $TEST_START_MARKER" >&2
    return 1
  fi

  if ! grep -F -q "$SUCCESS_MARKER" "$LOG_FILE_ABS"; then
    echo "FAILED: Log missing ticker success marker: $SUCCESS_MARKER" >&2
    return 1
  fi

  # Count task time reports, not IRQs: the sample also prints each task's
  # initial timestamp before its first timer-driven wakeup.
  local tick_count
  tick_count=$(grep -c -E "TA[123]  -" "$LOG_FILE_ABS" || true)
  if (( tick_count < 5 )); then
    echo "FAILED: Expected multiple ticker wakeups, found only $tick_count" >&2
    return 1
  fi

  echo "SUCCESS: RustSBI booted RTEMS S-mode ticker cleanly with $tick_count task time reports verified."
}

main() {
  download_image
  build_rustsbi
  run_boot_test
}

main "$@"
