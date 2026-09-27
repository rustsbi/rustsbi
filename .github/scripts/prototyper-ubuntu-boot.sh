#!/usr/bin/env bash
set -euo pipefail

# Boot the pinned Ubuntu image through the RustSBI build at the current checkout.
readonly UBUNTU_VERSION=24.04.5
readonly UBUNTU_IMAGE="ubuntu-${UBUNTU_VERSION}-preinstalled-server-riscv64.img"
readonly UBUNTU_SHA256=1cbd4b187f33356107daa637a5588d8f8a1744c7a012189e9c5e6d0b177825c4
readonly UBUNTU_URL="https://cdimage.ubuntu.com/releases/${UBUNTU_VERSION}/release/${UBUNTU_IMAGE}.xz"
readonly UBOOT_DEB=u-boot-qemu_2025.10-0ubuntu0.24.04.2_all.deb
readonly UBOOT_SHA256=2154e34e4c7037105e8448514faad296b2bbfb1cc460674e3d67cca42d402d9a
readonly EDK2_DEB=qemu-efi-riscv64_2024.02-2ubuntu0.9_all.deb
readonly EDK2_SHA256=40258b466ee56ea1e65a2990076e072488bc447430777c0ff120731a35477d18
readonly PACKAGE_MIRROR="${UBUNTU_PACKAGE_MIRROR:-https://archive.ubuntu.com/ubuntu}"

if (( $# != 1 )) || [[ "$1" != u-boot && "$1" != edk2 ]]; then
  echo "Usage: $0 <u-boot|edk2>" >&2
  exit 2
fi
readonly BOOT_MODE=$1
readonly CACHE_DIR="${UBUNTU_CACHE_DIR:-target/ubuntu-boot/cache/${UBUNTU_VERSION}}"
readonly IMAGE_XZ="${CACHE_DIR}/${UBUNTU_IMAGE}.xz"
readonly IMAGE_RAW="${CACHE_DIR}/${UBUNTU_IMAGE}"
readonly BUILD_DIR="${UBUNTU_BUILD_DIR:-target/ubuntu-boot}"
readonly RUSTSBI="target/riscv64gc-unknown-none-elf/release/rustsbi-prototyper-dynamic.bin"
readonly LOG_DIR="${QEMU_LOG_DIR:-qemu-logs}"
readonly LOG_FILE="${LOG_DIR}/prototyper-ubuntu-${BOOT_MODE}.log"
readonly BOOT_TIMEOUT_SECS="${UBUNTU_BOOT_TIMEOUT_SECS:-900}"

QEMU_PID=""
DOWNLOAD_TEMP=""
IMAGE_TEMP=""
RUN_DIR=""

cleanup() {
  if [[ -n "$QEMU_PID" ]] && kill -0 "$QEMU_PID" 2>/dev/null; then
    kill "$QEMU_PID" 2>/dev/null || true
    wait "$QEMU_PID" 2>/dev/null || true
  fi
  [[ -z "$DOWNLOAD_TEMP" ]] || rm -f "$DOWNLOAD_TEMP"
  [[ -z "$IMAGE_TEMP" ]] || rm -f "$IMAGE_TEMP"
  [[ -z "$RUN_DIR" ]] || rm -rf "$RUN_DIR"
}
trap cleanup EXIT

download_image() {
  mkdir -p "$CACHE_DIR"
  if [[ ! -f "$IMAGE_XZ" ]] || ! printf '%s  %s\n' "$UBUNTU_SHA256" "$IMAGE_XZ" | sha256sum -c --status; then
    DOWNLOAD_TEMP=$(mktemp "${IMAGE_XZ}.part.XXXXXX")
    curl --fail --location --retry 3 --retry-all-errors \
      --connect-timeout 30 --max-time 1800 \
      --output "$DOWNLOAD_TEMP" "$UBUNTU_URL"
    mv "$DOWNLOAD_TEMP" "$IMAGE_XZ"
    DOWNLOAD_TEMP=""
  fi
  printf '%s  %s\n' "$UBUNTU_SHA256" "$IMAGE_XZ" | sha256sum -c

  if [[ ! -s "$IMAGE_RAW" ]]; then
    IMAGE_TEMP=$(mktemp "${IMAGE_RAW}.part.XXXXXX")
    xz -dc "$IMAGE_XZ" >"$IMAGE_TEMP"
    mv "$IMAGE_TEMP" "$IMAGE_RAW"
    IMAGE_TEMP=""
  fi
}

prepare_bootloader() {
  local package sha path url
  if [[ "$BOOT_MODE" = u-boot ]]; then
    package=$UBOOT_DEB
    sha=$UBOOT_SHA256
    url="${PACKAGE_MIRROR}/pool/main/u/u-boot/${package}"
  else
    package=$EDK2_DEB
    sha=$EDK2_SHA256
    url="${PACKAGE_MIRROR}/pool/universe/e/edk2/${package}"
  fi
  path="${CACHE_DIR}/packages/${package}"
  mkdir -p "${CACHE_DIR}/packages" "$BUILD_DIR/$BOOT_MODE"
  if [[ ! -f "$path" ]] || ! printf '%s  %s\n' "$sha" "$path" | sha256sum -c --status; then
    DOWNLOAD_TEMP=$(mktemp "${path}.part.XXXXXX")
    curl --fail --location --retry 3 --retry-all-errors \
      --connect-timeout 30 --max-time 300 --output "$DOWNLOAD_TEMP" "$url"
    mv "$DOWNLOAD_TEMP" "$path"
    DOWNLOAD_TEMP=""
  fi
  printf '%s  %s\n' "$sha" "$path" | sha256sum -c
  dpkg-deb -x "$path" "$BUILD_DIR/$BOOT_MODE"
}

prepare_guest() {
  mkdir -p "$BUILD_DIR" "$LOG_DIR"
  RUN_DIR=$(mktemp -d "$(realpath "$BUILD_DIR")/run.XXXXXX")
  : >"$LOG_FILE"
  qemu-img create -q -f qcow2 -F raw -b "$(realpath "$IMAGE_RAW")" \
    "$RUN_DIR/ubuntu.qcow2"

  cat >"$RUN_DIR/user-data" <<'CLOUD_CONFIG'
#cloud-config
write_files:
  - path: /usr/local/sbin/rustsbi-ubuntu-smoke
    permissions: '0755'
    content: |
      #!/bin/sh
      set -eu
      trap 'echo RUSTSBI-UBUNTU-SMOKE-FAIL >/dev/ttyS0' EXIT
      test "$(uname -m)" = riscv64
      test "$(cat /proc/1/comm)" = systemd
      test -r /sys/firmware/devicetree/base/compatible
      trap - EXIT
      echo RUSTSBI-UBUNTU-SMOKE-OK >/dev/ttyS0
      poweroff -f
runcmd:
  - [/usr/local/sbin/rustsbi-ubuntu-smoke]
CLOUD_CONFIG
  printf 'instance-id: rustsbi-ubuntu-%s\nlocal-hostname: rustsbi-ubuntu\n' \
    "$BOOT_MODE" >"$RUN_DIR/meta-data"
  cloud-localds "$RUN_DIR/seed.img" "$RUN_DIR/user-data" "$RUN_DIR/meta-data"
}

start_qemu() {
  local -a args=(
    -nographic -no-reboot -smp 2 -m 4G
    -bios "$RUSTSBI"
    -drive "file=$RUN_DIR/ubuntu.qcow2,format=qcow2,if=none,id=hd0"
    -device "virtio-blk-device,drive=hd0"
    -drive "file=$RUN_DIR/seed.img,format=raw,if=none,id=seed0,readonly=on"
    -device "virtio-blk-device,drive=seed0"
    -netdev "user,id=net0"
    -device "virtio-net-device,netdev=net0"
    -device virtio-rng-pci
  )
  if [[ "$BOOT_MODE" = u-boot ]]; then
    args+=(-machine "virt,acpi=off"
      -kernel "$(realpath "$BUILD_DIR/u-boot/usr/lib/u-boot/qemu-riscv64_smode/uboot.elf")")
  else
    local fv_path
    fv_path="$(realpath "$BUILD_DIR/edk2")/usr/share/qemu-efi-riscv64"
    cp "$fv_path/RISCV_VIRT_CODE.fd" "$RUN_DIR/RISCV_VIRT_CODE.fd"
    cp "$fv_path/RISCV_VIRT_VARS.fd" "$RUN_DIR/RISCV_VIRT_VARS.fd"
    args+=(-machine "virt,pflash0=pflash0,pflash1=pflash1,acpi=off"
      -blockdev "node-name=pflash0,driver=file,read-only=on,filename=$RUN_DIR/RISCV_VIRT_CODE.fd"
      -blockdev "node-name=pflash1,driver=file,filename=$RUN_DIR/RISCV_VIRT_VARS.fd")
  fi
  qemu-system-riscv64 "${args[@]}" </dev/null >"$LOG_FILE" 2>&1 &
  QEMU_PID=$!
}

wait_for_ubuntu() {
  local second qemu_exit bootloader_banner
  if [[ "$BOOT_MODE" = u-boot ]]; then
    bootloader_banner='U-Boot 2025.10-0ubuntu0.24.04.2'
  else
    bootloader_banner='RISC-V EDK2 firmware version 2024.02-2ubuntu0.9'
  fi
  for ((second = 0; second < BOOT_TIMEOUT_SECS; second++)); do
    if grep -Fq RUSTSBI-UBUNTU-SMOKE-FAIL "$LOG_FILE"; then
      echo "Ubuntu smoke command failed" >&2
      return 1
    fi
    if grep -Eq 'Kernel panic|VFS: Unable to mount root fs|No bootable device' "$LOG_FILE"; then
      echo "Ubuntu reported a fatal boot error" >&2
      return 1
    fi
    if grep -Fq RUSTSBI-UBUNTU-SMOKE-OK "$LOG_FILE"; then
      if ! grep -Fq 'Hello RustSBI!' "$LOG_FILE" \
        || ! grep -Fq "$bootloader_banner" "$LOG_FILE" \
        || ! grep -Fq 'Linux version ' "$LOG_FILE"; then
        echo "Expected RustSBI, bootloader, or Linux banner is missing" >&2
        return 1
      fi
      return 0
    fi
    if ! kill -0 "$QEMU_PID" 2>/dev/null; then
      set +e
      wait "$QEMU_PID"
      qemu_exit=$?
      set -e
      echo "QEMU exited before Ubuntu smoke command (exit=${qemu_exit})" >&2
      return 1
    fi
    sleep 1
  done
  echo "Ubuntu did not reach userspace within ${BOOT_TIMEOUT_SECS}s" >&2
  return 1
}

main() {
  test -s "$RUSTSBI" || {
    echo "Missing ${RUSTSBI}; run 'cargo prototyper build' first" >&2
    return 1
  }
  download_image
  prepare_bootloader
  prepare_guest
  start_qemu
  if ! wait_for_ubuntu; then
    tail -n 160 "$LOG_FILE" >&2 || true
    return 1
  fi
  echo "RustSBI booted Ubuntu ${UBUNTU_VERSION} to userspace (${BOOT_MODE})"
  echo "QEMU serial log: ${LOG_FILE}"
}

main "$@"
