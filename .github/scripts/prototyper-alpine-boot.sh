#!/usr/bin/env bash
# Boot the official Alpine live image through the current RustSBI build.
set -euo pipefail

SCRIPT_PATH="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/$(basename "${BASH_SOURCE[0]}")"
readonly SCRIPT_PATH
TEST_MODE=boot
BOOT_MODE=
TEST_SCENARIO=
TEST_EXPECTED=
if (( $# == 1 )); then
  case "$1" in
    sbi | u-boot | edk2) BOOT_MODE=$1 ;;
    --self-test) TEST_MODE=self-test; BOOT_MODE=sbi ;;
    *)
      echo "Usage: $0 <sbi|u-boot|edk2> | --self-test" >&2
      exit 2
      ;;
  esac
elif (( $# == 3 )) && [[ "$1" = --self-test-case ]]; then
  TEST_MODE=self-test-case
  BOOT_MODE=sbi
  TEST_SCENARIO=$2
  TEST_EXPECTED=$3
else
  echo "Usage: $0 <sbi|u-boot|edk2> | --self-test" >&2
  exit 2
fi
readonly BOOT_MODE TEST_MODE
cd "$(dirname "${BASH_SOURCE[0]}")/../.."

readonly ALPINE_VERSION=3.23.3
readonly ALPINE_KERNEL=6.18.7-0-lts
readonly ISO_NAME="alpine-standard-${ALPINE_VERSION}-riscv64.iso"
readonly ISO_URL="https://dl-cdn.alpinelinux.org/alpine/v3.23/releases/riscv64/${ISO_NAME}"
readonly ISO_SHA256=3944efd920cdfb34d7f0b03b766ee549c6a82bac582315cfee55993490f2455f
readonly UBOOT_DEB=u-boot-qemu_2025.10-0ubuntu0.24.04.2_all.deb
readonly UBOOT_SHA256=2154e34e4c7037105e8448514faad296b2bbfb1cc460674e3d67cca42d402d9a
readonly EDK2_DEB=qemu-efi-riscv64_2024.02-2ubuntu0.10_all.deb
readonly EDK2_SHA256=4f2a7b3757906f60eeedca3181e8d83af6f45477ecc9f306f483cebf665ecdd4
readonly PACKAGE_MIRROR=https://archive.ubuntu.com/ubuntu
readonly CACHE_DIR="${ALPINE_CACHE_DIR:-target/alpine-boot/cache}"
readonly WORK_BASE="${ALPINE_WORK_DIR:-target/alpine-boot/work}"
readonly LOG_DIR="${QEMU_LOG_DIR:-qemu-logs}"
readonly LOG_FILE="${LOG_DIR}/prototyper-alpine-${BOOT_MODE}.log"
readonly RUSTSBI="${ALPINE_RUSTSBI:-target/riscv64gc-unknown-none-elf/release/rustsbi-prototyper-dynamic.bin}"
readonly QEMU="${ALPINE_QEMU:-qemu-system-riscv64}"
readonly BOOT_TIMEOUT_SECS="${ALPINE_BOOT_TIMEOUT_SECS:-300}"
readonly DOWNLOAD_TIMEOUT_SECS="${ALPINE_DOWNLOAD_TIMEOUT_SECS:-900}"
readonly SMOKE_MARKER=RUSTSBI-ALPINE-SMOKE-OK
readonly FAILURE_PATTERN='Kernel panic|not syncing|Attempted to kill init|panicked at|Synchronous Exception|Unhandled exception|grub rescue>|RUSTSBI-ALPINE-SMOKE-FAIL'
readonly SERIAL_CR=$'\r'

QEMU_PID=""
DOWNLOAD_TEMP=""
RUN_DIR=""
cleanup() {
  if [[ -n "$QEMU_PID" ]] && kill -0 "$QEMU_PID" 2>/dev/null; then
    kill "$QEMU_PID" 2>/dev/null || true
    wait "$QEMU_PID" 2>/dev/null || true
  fi
  exec 3>&-
  [[ -z "$DOWNLOAD_TEMP" ]] || rm -f "$DOWNLOAD_TEMP"
  [[ -z "$RUN_DIR" ]] || rm -rf "$RUN_DIR"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

verify_sha256() {
  printf '%s  %s\n' "$1" "$2" | sha256sum --check --status
}

download_asset() {
  local url=$1 destination=$2 digest=$3
  if [[ -f "$destination" ]] && verify_sha256 "$digest" "$destination"; then
    echo "Using verified $(basename "$destination")"
    return
  fi
  DOWNLOAD_TEMP=$(mktemp "${destination}.part.XXXXXX")
  curl --fail --location --connect-timeout 30 --max-time "$DOWNLOAD_TIMEOUT_SECS" \
    --retry 3 --retry-all-errors --retry-max-time "$DOWNLOAD_TIMEOUT_SECS" \
    --output "$DOWNLOAD_TEMP" "$url"
  if ! verify_sha256 "$digest" "$DOWNLOAD_TEMP"; then
    echo "SHA-256 mismatch: $url" >&2
    return 1
  fi
  mv "$DOWNLOAD_TEMP" "$destination"
  DOWNLOAD_TEMP=""
}

prepare_assets() {
  local package digest url
  download_asset "$ISO_URL" "$CACHE_DIR/$ISO_NAME" "$ISO_SHA256"
  if [[ "$BOOT_MODE" = sbi ]]; then
    xorriso -osirrox on -indev "$CACHE_DIR/$ISO_NAME" \
      -extract /boot/vmlinuz-lts "$RUN_DIR/vmlinuz-lts" \
      -extract /boot/initramfs-lts "$RUN_DIR/initramfs-lts"
    gzip -dc "$RUN_DIR/vmlinuz-lts" > "$RUN_DIR/Image"
  else
    if [[ "$BOOT_MODE" = u-boot ]]; then
      package=$UBOOT_DEB
      digest=$UBOOT_SHA256
      url="$PACKAGE_MIRROR/pool/main/u/u-boot/$package"
    else
      package=$EDK2_DEB
      digest=$EDK2_SHA256
      url="$PACKAGE_MIRROR/pool/universe/e/edk2/$package"
    fi
    download_asset "$url" "$CACHE_DIR/$package" "$digest"
    dpkg-deb -x "$CACHE_DIR/$package" "$RUN_DIR/bootloader"
  fi
}

start_qemu() {
  local -a args=(
    -nographic -monitor none -no-reboot -smp 2 -m 2G
    -bios "$RUSTSBI"
    -drive "file=$CACHE_DIR/$ISO_NAME,format=raw,if=none,id=cd0,readonly=on"
    -device 'virtio-blk-device,drive=cd0'
    -device virtio-rng-device
  )
  case "$BOOT_MODE" in
    sbi)
      args+=(-machine 'virt,acpi=off'
        -kernel "$RUN_DIR/Image" -initrd "$RUN_DIR/initramfs-lts"
        -append 'console=ttyS0 modules=loop,squashfs,sd-mod,usb-storage quiet')
      ;;
    u-boot)
      args+=(-machine 'virt,acpi=off'
        -kernel "$RUN_DIR/bootloader/usr/lib/u-boot/qemu-riscv64_smode/uboot.elf")
      ;;
    edk2)
      # Each run gets fresh UEFI variables; the CODE volume remains read-only.
      args+=(-machine 'virt,pflash0=pflash0,pflash1=pflash1,acpi=off'
        -blockdev "node-name=pflash0,driver=file,read-only=on,filename=$RUN_DIR/bootloader/usr/share/qemu-efi-riscv64/RISCV_VIRT_CODE.fd"
        -blockdev "node-name=pflash1,driver=file,filename=$RUN_DIR/bootloader/usr/share/qemu-efi-riscv64/RISCV_VIRT_VARS.fd")
      ;;
  esac
  {
    printf 'QEMU command: '
    printf '%q ' "$QEMU" "${args[@]}"
    printf '\n'
  } >> "$LOG_FILE"
  mkfifo "$RUN_DIR/input"
  # Keep both ends open while QEMU waits for the login and smoke commands.
  exec 3<> "$RUN_DIR/input"
  "$QEMU" "${args[@]}" < "$RUN_DIR/input" >> "$LOG_FILE" 2>&1 &
  QEMU_PID=$!
}

check_boot_markers() {
  grep -Fq 'Hello RustSBI!' "$LOG_FILE" &&
    grep -Fq 'OpenRC 0.63 is starting up Linux' "$LOG_FILE" &&
    grep -Fq "Kernel $ALPINE_KERNEL on riscv64 (/dev/ttyS0)" "$LOG_FILE" &&
    grep -Fq 'Welcome to Alpine Linux 3.23' "$LOG_FILE" || return 1
  case "$BOOT_MODE" in
    u-boot)
      grep -Fq 'U-Boot 2025.10-0ubuntu0.24.04.2' "$LOG_FILE" &&
        grep -Fq "Booting \`Linux lts'" "$LOG_FILE"
      ;;
    edk2)
      grep -Fq 'RISC-V EDK2 firmware version 2024.02-2ubuntu0.10' "$LOG_FILE" &&
        grep -Fq "Booting \`Linux lts'" "$LOG_FILE"
      ;;
  esac
}

send_smoke_command() {
  local efi_check='test -d /sys/firmware/efi' command
  [[ "$BOOT_MODE" != sbi ]] || efi_check='test ! -d /sys/firmware/efi'
  # Split the marker's format string so terminal echo cannot satisfy the check.
  # shellcheck disable=SC2016 # Variables in this command expand in the guest.
  printf -v command 'if . /etc/os-release && [ "$ID" = alpine ] && [ "$(cat /etc/alpine-release)" = "%s" ] && [ "$(uname -m)" = riscv64 ] && [ "$(uname -r)" = "%s" ] && [ "$(cat /proc/1/comm)" = init ] && test -r /sys/firmware/devicetree/base/compatible && apk --version && %s; then printf "\\nRUSTSBI-ALPINE-SMOKE-%%s %%s\\n" OK %s; poweroff -f; else printf "\\nRUSTSBI-ALPINE-SMOKE-%%s %%s\\n" FAIL %s; fi' \
    "$ALPINE_VERSION" "$ALPINE_KERNEL" "$efi_check" "$BOOT_MODE" "$BOOT_MODE"
  printf '%s\n' "$command" >&3
}

wait_for_guest() {
  local deadline=$((SECONDS + BOOT_TIMEOUT_SECS)) stage=login qemu_exit
  while (( SECONDS < deadline )); do
    if grep -Eq "$FAILURE_PATTERN" "$LOG_FILE"; then
      echo "Alpine reported a fatal boot or smoke-test error" >&2
      return 1
    fi
    # Match an entire output line, allowing serial CRs before LF.
    if grep -Exq "$SMOKE_MARKER $BOOT_MODE${SERIAL_CR}*" "$LOG_FILE"; then
      if [[ "$stage" != smoke && "$stage" != shutdown ]]; then
        echo "Alpine success marker appeared before the smoke command" >&2
        return 1
      fi
      if ! check_boot_markers; then
        echo "Expected firmware, bootloader or Alpine boot evidence is missing" >&2
        return 1
      fi
      stage=shutdown
    fi
    if ! kill -0 "$QEMU_PID" 2>/dev/null; then
      qemu_exit=0
      wait "$QEMU_PID" || qemu_exit=$?
      QEMU_PID=""
      if [[ "$stage" = shutdown && "$qemu_exit" = 0 ]]; then
        return 0
      fi
      echo "QEMU exited before successful Alpine shutdown (exit=$qemu_exit)" >&2
      return 1
    fi
    if [[ "$stage" = login ]] && grep -Fq 'localhost login:' "$LOG_FILE"; then
      printf 'root\n' >&3
      stage=shell
    elif [[ "$stage" = shell ]] && grep -Fq 'localhost:~# ' "$LOG_FILE"; then
      send_smoke_command
      stage=smoke
    fi
    sleep 1
  done
  echo "Alpine timed out after ${BOOT_TIMEOUT_SECS}s (stage=$stage)" >&2
  return 1
}

self_test_case() {
  local scenario=$1 expected=$2 result=0
  mkdir -p "$LOG_DIR"
  RUN_DIR=$(mktemp -d "${TMPDIR:-/tmp}/rustsbi-alpine-test.XXXXXX")
  mkfifo "$RUN_DIR/input"
  exec 3<> "$RUN_DIR/input"
  (
    if [[ "$scenario" != missing-firmware ]]; then
      printf 'Hello RustSBI!\n'
    fi
    printf '%s\n' \
      'OpenRC 0.63 is starting up Linux 6.18.7-0-lts (riscv64)' \
      'Welcome to Alpine Linux 3.23' \
      'Kernel 6.18.7-0-lts on riscv64 (/dev/ttyS0)'
    if [[ "$scenario" = premature-marker ]]; then
      printf 'RUSTSBI-ALPINE-SMOKE-OK sbi\n'
      exit 0
    fi
    printf 'localhost login: '
    IFS= read -r login
    [[ "$login" = root ]] || exit 5
    printf '\nlocalhost:~# '
    IFS= read -r command
    [[ "$command" = 'if . /etc/os-release'* ]] || exit 6
    printf '\n'
    case "$scenario" in
      success) printf 'RUSTSBI-ALPINE-SMOKE-OK sbi\n' ;;
      serial-cr) printf 'RUSTSBI-ALPINE-SMOKE-OK sbi\r\r\n' ;;
      missing-marker) printf 'apk-tools 3.0\n' ;;
      echoed-marker) printf 'localhost:~# echo RUSTSBI-ALPINE-SMOKE-OK sbi\n' ;;
      wrong-path) printf 'RUSTSBI-ALPINE-SMOKE-OK edk2\n' ;;
      missing-firmware) printf 'RUSTSBI-ALPINE-SMOKE-OK sbi\n' ;;
      guest-failure) printf 'RUSTSBI-ALPINE-SMOKE-FAIL sbi\n' ;;
      panic)
        printf 'RUSTSBI-ALPINE-SMOKE-OK sbi\nKernel panic - not syncing\n'
        ;;
      nonzero-exit) printf 'RUSTSBI-ALPINE-SMOKE-OK sbi\n'; exit 7 ;;
      timeout) sleep 10 ;;
      shutdown-timeout) printf 'RUSTSBI-ALPINE-SMOKE-OK sbi\n'; sleep 10 ;;
      *) exit 4 ;;
    esac
  ) < "$RUN_DIR/input" > "$LOG_FILE" 2>&1 &
  QEMU_PID=$!
  wait_for_guest > "$LOG_DIR/result.txt" 2>&1 || result=$?
  if [[ "$expected" = pass && "$result" != 0 ]] ||
    [[ "$expected" = fail && "$result" = 0 ]]; then
    echo "FAIL: $scenario (expected=$expected, exit=$result)" >&2
    cat "$LOG_DIR/result.txt" "$LOG_FILE" >&2
    return 1
  fi
  test -s "$LOG_FILE"
  cleanup
  test -s "$LOG_FILE"
  echo "PASS: $scenario ($expected, serial log retained)"
}

self_test() {
  local test_dir scenario expected timeout status=0
  test_dir=$(mktemp -d)
  for scenario in success serial-cr premature-marker missing-marker echoed-marker \
    wrong-path missing-firmware guest-failure panic nonzero-exit timeout shutdown-timeout; do
    expected=fail
    [[ "$scenario" = success || "$scenario" = serial-cr ]] && expected=pass
    timeout=10
    [[ "$scenario" = timeout || "$scenario" = shutdown-timeout ]] && timeout=4
    if ! QEMU_LOG_DIR="$test_dir/$scenario" ALPINE_BOOT_TIMEOUT_SECS="$timeout" \
      "$SCRIPT_PATH" --self-test-case "$scenario" "$expected"; then
      status=1
    fi
  done
  rm -rf "$test_dir"
  return "$status"
}

main() {
  [[ "$BOOT_TIMEOUT_SECS" =~ ^[1-9][0-9]*$ ]] || {
    echo "ALPINE_BOOT_TIMEOUT_SECS must be a positive integer" >&2
    return 2
  }
  mkdir -p "$CACHE_DIR" "$WORK_BASE" "$LOG_DIR"
  : > "$LOG_FILE"
  test -s "$RUSTSBI" || {
    echo "Missing $RUSTSBI; run 'cargo prototyper build' first" >&2
    return 1
  }
  "$QEMU" --version | tee -a "$LOG_FILE"
  sha256sum "$RUSTSBI" | tee -a "$LOG_FILE"
  RUN_DIR=$(mktemp -d "$(realpath "$WORK_BASE")/$BOOT_MODE.XXXXXX")
  prepare_assets
  start_qemu
  if ! wait_for_guest; then
    tail -n 100 "$LOG_FILE" >&2 || true
    return 1
  fi
  echo "RustSBI booted Alpine $ALPINE_VERSION to userspace ($BOOT_MODE)"
  echo "QEMU serial log: $LOG_FILE"
}

if [[ "${BASH_SOURCE[0]}" = "$0" ]]; then
  case "$TEST_MODE" in
    boot) main ;;
    self-test) self_test ;;
    self-test-case) self_test_case "$TEST_SCENARIO" "$TEST_EXPECTED" ;;
  esac
fi
