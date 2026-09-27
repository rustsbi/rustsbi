#!/usr/bin/env bash
set -euo pipefail

readonly REDOX_RELEASE="rustsbi-ci-2026-09-27"
readonly REDOX_IMAGE="redox_server_riscv64gc_rustsbi-ci_harddrive.img.zst"
readonly REDOX_IMAGE_SHA256="a49e0af51059e609c1d2380aa59b310566a2b4993b46397f5cbcdd1dfddd98a9"
readonly REDOX_BASE_URL="https://github.com/THRAI/redox/releases/download/${REDOX_RELEASE}"
readonly EDK2_VERSION="v10.2.1"
readonly EDK2_BASE_URL="https://raw.githubusercontent.com/qemu/qemu/${EDK2_VERSION}/pc-bios"
readonly EDK2_CODE_ARCHIVE="edk2-riscv-code.fd.bz2"
readonly EDK2_CODE_ARCHIVE_SHA256="e33120b298f76f3658ec9daa7ba7f579062e60decf75cd746a933625b8b76167"
readonly EDK2_CODE_SHA256="c3b779f86671c0cadf9cbdb274917a20210aeceb6f58b42d68406cdddfa48869"
readonly EDK2_VARS_ARCHIVE="edk2-riscv-vars.fd.bz2"
readonly EDK2_VARS_ARCHIVE_SHA256="881b87567b3a20c8363c602c56b688ccfc0fb83683dab23acfcbb5eadb0b1e09"
readonly EDK2_VARS_SHA256="ea8094e953b1215444bd001ee1cf22818f1f7f8abcb158e62180cbaa6c1f70af"

readonly CACHE_DIR="${REDOX_CACHE_DIR:-.cache/redox/${REDOX_RELEASE}}"
readonly WORK_DIR="${REDOX_WORK_DIR:-.redox-ci}"
readonly LOG_DIR="${QEMU_LOG_DIR:-qemu-logs}"
readonly LOG_FILE="${LOG_DIR}/prototyper-redox.log"
readonly QEMU_DEBUG_LOG="${LOG_DIR}/prototyper-redox-qemu-debug.log"
readonly BOOT_TIMEOUT_SECS="${REDOX_BOOT_TIMEOUT_SECS:-300}"
readonly DOWNLOAD_CONNECT_TIMEOUT_SECS="${REDOX_DOWNLOAD_CONNECT_TIMEOUT_SECS:-30}"
readonly DOWNLOAD_TIMEOUT_SECS="${REDOX_DOWNLOAD_TIMEOUT_SECS:-900}"
readonly RUSTSBI="${REDOX_RUSTSBI:-target/riscv64gc-unknown-none-elf/release/rustsbi-prototyper-dynamic.elf}"
readonly SMOKE_MARKER="RUSTSBI-REDOX-SMOKE-OK"
readonly PFLASH_SIZE=33554432

readonly ARCHIVE_PATH="${CACHE_DIR}/${REDOX_IMAGE}"
readonly DISK_IMAGE="${WORK_DIR}/redox.raw"
readonly DISK_OVERLAY="${WORK_DIR}/redox.qcow2"
readonly EXTRA_DISK="${WORK_DIR}/extra.raw"
readonly CODE_COPY="${WORK_DIR}/edk2-code.fd"
readonly VARS_COPY="${WORK_DIR}/edk2-vars.fd"
readonly QEMU_INPUT="${WORK_DIR}/qemu-input"

QEMU_PID=""
DOWNLOAD_TEMP=""

verify_archive() {
  (
    cd "$CACHE_DIR"
    printf '%s  %s\n' "$REDOX_IMAGE_SHA256" "$REDOX_IMAGE" | sha256sum --check
  )
}

download_file() {
  local url=$1
  local destination=$2
  DOWNLOAD_TEMP=$(mktemp "${destination}.part.XXXXXX")
  curl --fail --location \
    --connect-timeout "$DOWNLOAD_CONNECT_TIMEOUT_SECS" \
    --max-time "$DOWNLOAD_TIMEOUT_SECS" \
    --retry 3 \
    --retry-all-errors \
    --retry-max-time "$DOWNLOAD_TIMEOUT_SECS" \
    --output "$DOWNLOAD_TEMP" \
    "$url"
  mv "$DOWNLOAD_TEMP" "$destination"
  DOWNLOAD_TEMP=""
}

download_archive() {
  mkdir -p "$CACHE_DIR"
  if verify_archive >/dev/null 2>&1; then
    verify_archive
    return
  fi

  download_file "${REDOX_BASE_URL}/${REDOX_IMAGE}" "$ARCHIVE_PATH"
  verify_archive
}

prepare_edk2_image() {
  local archive=$1
  local archive_sha256=$2
  local image_sha256=$3
  local destination=$4
  local archive_path="${CACHE_DIR}/${archive}"

  if ! printf '%s  %s\n' "$archive_sha256" "$archive_path" \
    | sha256sum --check >/dev/null 2>&1; then
    download_file "${EDK2_BASE_URL}/${archive}" "$archive_path"
  fi
  printf '%s  %s\n' "$archive_sha256" "$archive_path" | sha256sum --check
  bzip2 --decompress --stdout "$archive_path" >"$destination"
  printf '%s  %s\n' "$image_sha256" "$destination" | sha256sum --check
}

copy_pflash() {
  local source=$1
  local destination=$2
  local size
  size=$(wc -c <"$source")
  if ((size > PFLASH_SIZE)); then
    echo "EDK II image is larger than the QEMU virt pflash: $source" >&2
    return 1
  fi
  cp "$source" "$destination"
  truncate -s "$PFLASH_SIZE" "$destination"
}

prepare_assets() {
  local code_source
  local vars_source

  mkdir -p "$WORK_DIR" "$LOG_DIR"
  download_archive
  zstd --decompress --force "$ARCHIVE_PATH" -o "$DISK_IMAGE"

  code_source=${REDOX_EDK2_CODE:-}
  if [[ -z "$code_source" ]]; then
    prepare_edk2_image \
      "$EDK2_CODE_ARCHIVE" \
      "$EDK2_CODE_ARCHIVE_SHA256" \
      "$EDK2_CODE_SHA256" \
      "$CODE_COPY"
  else
    copy_pflash "$code_source" "$CODE_COPY"
  fi

  vars_source=${REDOX_EDK2_VARS:-}
  if [[ -z "$vars_source" ]]; then
    prepare_edk2_image \
      "$EDK2_VARS_ARCHIVE" \
      "$EDK2_VARS_ARCHIVE_SHA256" \
      "$EDK2_VARS_SHA256" \
      "$VARS_COPY"
  else
    copy_pflash "$vars_source" "$VARS_COPY"
  fi

  rm -f "$DISK_OVERLAY" "$EXTRA_DISK" "$QEMU_INPUT"
  qemu-img create -q -f qcow2 -F raw -b "$(realpath "$DISK_IMAGE")" "$DISK_OVERLAY"
  qemu-img create -q -f raw "$EXTRA_DISK" 1G
  mkfifo "$QEMU_INPUT"
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
  exec 3<>"$QEMU_INPUT"
  qemu-system-riscv64 \
    -name "Redox RustSBI CI" \
    -machine virt,acpi=off,aia=none \
    -accel tcg \
    -cpu max \
    -smp 4 \
    -m 2048 \
    -no-reboot \
    -bios "$RUSTSBI" \
    -drive "if=pflash,format=raw,unit=0,readonly=on,file=${CODE_COPY}" \
    -drive "if=pflash,format=raw,unit=1,file=${VARS_COPY}" \
    -vga none \
    -device ramfb \
    -device qemu-xhci \
    -device usb-kbd \
    -device usb-tablet \
    -drive "if=none,id=drv0,format=qcow2,file=${DISK_OVERLAY}" \
    -device nvme,drive=drv0,serial=NVME_SERIAL \
    -drive "if=none,id=drv1,format=raw,file=${EXTRA_DISK}" \
    -device nvme,drive=drv1,serial=NVME_EXTRA \
    -display none \
    -monitor none \
    -serial stdio \
    -d guest_errors \
    -D "$QEMU_DEBUG_LOG" \
    <"$QEMU_INPUT" >"$LOG_FILE" 2>&1 &
  QEMU_PID=$!
}

guest_failed() {
  grep -aEq \
    'timeout on init|panicked at|Kernel panic|Exception: (Load|Store|Instruction)PageFault|not able to mount uuid' \
    "$LOG_FILE"
}

report_failure() {
  local reason=$1
  echo "$reason" >&2
  tail -n 200 "$LOG_FILE" >&2 || true
}

wait_for_text() {
  local text=$1
  local description=$2
  local deadline=$((SECONDS + BOOT_TIMEOUT_SECS))

  while ((SECONDS < deadline)); do
    if grep -aFq "$text" "$LOG_FILE"; then
      return
    fi
    if guest_failed; then
      report_failure "Redox reported an early boot failure while waiting for ${description}"
      return 1
    fi
    if ! kill -0 "$QEMU_PID" 2>/dev/null; then
      report_failure "QEMU exited while waiting for ${description}"
      return 1
    fi
    sleep 1
  done

  report_failure "Timed out waiting for ${description}"
  return 1
}

smoke_marker_is_an_output_line() {
  tr -d '\r' <"$LOG_FILE" | grep -aFxq "$SMOKE_MARKER"
}

wait_for_smoke_marker() {
  local deadline=$((SECONDS + BOOT_TIMEOUT_SECS))

  while ((SECONDS < deadline)); do
    if smoke_marker_is_an_output_line; then
      return
    fi
    if guest_failed; then
      report_failure "Redox reported a failure before the smoke command completed"
      return 1
    fi
    if ! kill -0 "$QEMU_PID" 2>/dev/null; then
      report_failure "QEMU exited before the smoke command completed"
      return 1
    fi
    sleep 1
  done

  report_failure "Timed out waiting for the smoke command output"
  return 1
}

send_serial_line() {
  printf '%s\r' "$1" >&3
}

verify_boot_log() {
  grep -aF 'Hello RustSBI!' "$LOG_FILE"
  grep -aF 'PLIC: using context 1' "$LOG_FILE"
  grep -aF 'Serial: NVME_SERIAL' "$LOG_FILE"
  grep -aF 'NSID: 1 Size: 1048576 Capacity: 1048576' "$LOG_FILE"
  grep -aF 'init: switchroot to /usr /etc' "$LOG_FILE"
  grep -aF 'Welcome to Redox OS!' "$LOG_FILE"
  smoke_marker_is_an_output_line

  if guest_failed; then
    echo "Failure signature found after the smoke test" >&2
    return 1
  fi
}

main() {
  trap cleanup EXIT
  test -s "$RUSTSBI"
  qemu-system-riscv64 --version
  prepare_assets
  start_qemu

  wait_for_text 'Arrow keys and enter select mode' 'the Redox resolution menu'
  send_serial_line ''
  wait_for_text 'redox login:' 'the Redox login prompt'
  send_serial_line 'user'
  wait_for_text 'user: /home/user' 'the Redox user shell'
  send_serial_line "echo ${SMOKE_MARKER}"
  wait_for_smoke_marker

  verify_boot_log
  echo "RustSBI booted Redox and completed its userspace smoke test"
  echo "QEMU log: ${LOG_FILE}"
}

main "$@"
