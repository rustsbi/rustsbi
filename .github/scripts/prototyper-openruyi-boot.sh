#!/usr/bin/env bash
set -euo pipefail

readonly RELEASE="2026.08"
readonly BASE_URL="https://releases.openruyi.cn/creek/${RELEASE}/rva23"
readonly KERNEL_NAME="openRuyi-${RELEASE}-zero.kernel"
readonly INITRD_NAME="openRuyi-${RELEASE}-zero.cpio.gz"
readonly KERNEL_SHA256="360d46888bd042078da55d6e25f030a5fb7a5ba3c995391fcf05b591a95973e2"
readonly INITRD_SHA256="0ed99c5e1d485db52df4f63c85bed66ef287fbc49c66bcc62100ebb0a69d6c91"

readonly CACHE_DIR="${OPENRUYI_CACHE_DIR:-.cache/openruyi/${RELEASE}}"
readonly KERNEL_PATH="${CACHE_DIR}/${KERNEL_NAME}"
readonly INITRD_PATH="${CACHE_DIR}/${INITRD_NAME}"
readonly BIOS="target/riscv64gc-unknown-none-elf/release/rustsbi-prototyper-dynamic.elf"
readonly LOG_DIR="${QEMU_LOG_DIR:-qemu-logs}"
readonly LOG_FILE="${LOG_DIR}/prototyper-openruyi.log"
readonly BOOT_TIMEOUT_SECS="${OPENRUYI_BOOT_TIMEOUT_SECS:-240}"
readonly DOWNLOAD_CONNECT_TIMEOUT_SECS="${OPENRUYI_DOWNLOAD_CONNECT_TIMEOUT_SECS:-30}"
readonly DOWNLOAD_TIMEOUT_SECS="${OPENRUYI_DOWNLOAD_TIMEOUT_SECS:-900}"

QEMU_PID=""
DOWNLOAD_TEMP=""

verify_assets() {
  (
    cd "$CACHE_DIR"
    printf '%s  %s\n%s  %s\n' \
      "$KERNEL_SHA256" "$KERNEL_NAME" \
      "$INITRD_SHA256" "$INITRD_NAME" \
      | sha256sum --check
  )
}

download_asset() {
  local name=$1
  local destination=$2
  DOWNLOAD_TEMP=$(mktemp "${destination}.part.XXXXXX")

  if ! curl --fail --location \
    --connect-timeout "$DOWNLOAD_CONNECT_TIMEOUT_SECS" \
    --max-time "$DOWNLOAD_TIMEOUT_SECS" \
    --retry 3 \
    --retry-all-errors \
    --retry-max-time "$DOWNLOAD_TIMEOUT_SECS" \
    --output "$DOWNLOAD_TEMP" \
    "${BASE_URL}/${name}"; then
    return 1
  fi

  mv "$DOWNLOAD_TEMP" "$destination"
  DOWNLOAD_TEMP=""
}

prepare_assets() {
  mkdir -p "$CACHE_DIR"
  if verify_assets >/dev/null 2>&1; then
    verify_assets
    return
  fi

  echo "Downloading openRuyi ${RELEASE} Zero boot assets"
  download_asset "$KERNEL_NAME" "$KERNEL_PATH"
  download_asset "$INITRD_NAME" "$INITRD_PATH"
  verify_assets
}

check_qemu() {
  test -s "$BIOS"
  qemu-system-riscv64 --version
  if ! qemu-system-riscv64 -cpu help | awk '$1 == "rva23s64" { found = 1 } END { exit !found }'; then
    echo "qemu-system-riscv64 does not provide the required rva23s64 CPU" >&2
    return 1
  fi
}

stop_qemu() {
  if [[ -n "$QEMU_PID" ]] && kill -0 "$QEMU_PID" 2>/dev/null; then
    kill "$QEMU_PID" 2>/dev/null || true
    wait "$QEMU_PID" 2>/dev/null || true
  fi
}

cleanup() {
  if [[ -n "$DOWNLOAD_TEMP" ]]; then
    rm -f "$DOWNLOAD_TEMP"
  fi
  stop_qemu
}

start_qemu() {
  mkdir -p "$LOG_DIR"
  qemu-system-riscv64 \
    -machine virt \
    -cpu rva23s64,pmp=true \
    -smp 1 \
    -m 4G \
    -nographic \
    -no-reboot \
    -bios "$BIOS" \
    -kernel "$KERNEL_PATH" \
    -initrd "$INITRD_PATH" \
    >"$LOG_FILE" 2>&1 &
  QEMU_PID=$!
}

userspace_is_ready() {
  grep -Fq 'Hello RustSBI!' "$LOG_FILE" \
    && grep -Fq 'Welcome to openRuyi' "$LOG_FILE"
}

report_early_exit() {
  local qemu_exit
  set +e
  wait "$QEMU_PID"
  qemu_exit=$?
  set -e

  echo "QEMU exited before openRuyi reached userspace (exit=${qemu_exit})" >&2
  tail -n 120 "$LOG_FILE" || true
}

wait_for_userspace() {
  local elapsed
  for ((elapsed = 0; elapsed < BOOT_TIMEOUT_SECS; elapsed++)); do
    if userspace_is_ready; then
      return 0
    fi
    if ! kill -0 "$QEMU_PID" 2>/dev/null; then
      report_early_exit
      return 1
    fi
    sleep 1
  done

  echo "openRuyi did not reach userspace within ${BOOT_TIMEOUT_SECS}s" >&2
  tail -n 120 "$LOG_FILE" || true
  return 1
}

main() {
  trap cleanup EXIT
  prepare_assets
  check_qemu
  start_qemu
  wait_for_userspace

  echo "RustSBI booted openRuyi ${RELEASE} Zero successfully"
  echo "QEMU log: ${LOG_FILE}"
}

main "$@"
