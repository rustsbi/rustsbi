#!/usr/bin/env bash
set -euo pipefail

readonly DEBIAN_RELEASE="13"
readonly DEBIAN_BUILD="20260914-2601"
readonly DEBIAN_IMAGE="debian-13-nocloud-riscv64-${DEBIAN_BUILD}.qcow2"
readonly DEBIAN_BASE_URL="https://cloud.debian.org/images/cloud/trixie/${DEBIAN_BUILD}"
readonly DEBIAN_SHA512="fec8132ba6e6bce814cbc1a9ef0d2af59caae92cd279a5c99a615876735563f50f2dcb7cd7bb9f36ff4572bcf839433390fd44981d319d98fa525a3f29eed393"
readonly DEBIAN_KERNEL_RELEASE="6.12.107+deb13-riscv64"
readonly DEBIAN_ROOT_UUID="29f86a99-971c-447f-9389-4fa5d6220487"
readonly DEBIAN_ROOT_PARTUUID="e2e9340e-0027-4d1c-9319-7f6b7fb02c28"
readonly KERNEL_OPTIONS="root=PARTUUID=${DEBIAN_ROOT_PARTUUID} ro systemd.firstboot=off"
readonly UBOOT_REPOSITORY="https://github.com/u-boot/u-boot.git"
readonly UBOOT_VERSION="v2026.07"
readonly UBOOT_COMMIT="ece349ade2973e220f524ce59e59711cc919263f"

if (( $# > 1 )); then
  echo "Usage: $0 [sbi|u-boot]" >&2
  exit 2
fi
readonly BOOT_MODE="${1:-u-boot}"
case "$BOOT_MODE" in
  sbi | u-boot) ;;
  *)
    echo "Unknown Debian boot mode: ${BOOT_MODE}" >&2
    echo "Usage: $0 [sbi|u-boot]" >&2
    exit 2
    ;;
esac

readonly DEBIAN_CACHE_DIR="${DEBIAN_CACHE_DIR:-.cache/debian/${DEBIAN_RELEASE}}"
readonly DEBIAN_IMAGE_PATH="${DEBIAN_CACHE_DIR}/${DEBIAN_IMAGE}"
readonly UBOOT_CACHE_DIR="${UBOOT_CACHE_DIR:-.cache/u-boot/${UBOOT_VERSION}}"
readonly UBOOT_BUILD_DIR="${UBOOT_BUILD_DIR:-target/u-boot-debian}"
readonly RUSTSBI="target/riscv64gc-unknown-none-elf/release/rustsbi-prototyper-dynamic.bin"
readonly LOG_DIR="${QEMU_LOG_DIR:-qemu-logs}"
readonly LOG_FILE="${LOG_DIR}/prototyper-debian-${BOOT_MODE}.log"
readonly BOOT_TIMEOUT_SECS="${DEBIAN_BOOT_TIMEOUT_SECS:-600}"
readonly DOWNLOAD_CONNECT_TIMEOUT_SECS="${DEBIAN_DOWNLOAD_CONNECT_TIMEOUT_SECS:-30}"
readonly DOWNLOAD_TIMEOUT_SECS="${DEBIAN_DOWNLOAD_TIMEOUT_SECS:-1800}"
readonly BOOT_COMMAND="ext4load virtio 0:1 \${kernel_addr_r} /boot/vmlinux-${DEBIAN_KERNEL_RELEASE}; ext4load virtio 0:1 \${ramdisk_addr_r} /boot/initrd.img-${DEBIAN_KERNEL_RELEASE}; setenv bootargs ${KERNEL_OPTIONS}; booti \${kernel_addr_r} \${ramdisk_addr_r}:\${filesize} \${fdtcontroladdr}"

QEMU_PID=""
DOWNLOAD_TEMP=""
QEMU_INPUT=""
QEMU_OVERLAY=""
NBD_DEVICE=""
DEBIAN_MOUNT_DIR=""
DEBIAN_MOUNTED=false
DEBIAN_BOOT_DIR=""

verify_debian_image() {
  local actual_sha512
  [[ -f "$DEBIAN_IMAGE_PATH" ]] || return 1
  actual_sha512=$(sha512sum "$DEBIAN_IMAGE_PATH")
  actual_sha512=${actual_sha512%% *}

  if [[ "$actual_sha512" != "$DEBIAN_SHA512" ]]; then
    echo "Debian image checksum mismatch" >&2
    echo "expected: ${DEBIAN_SHA512}" >&2
    echo "actual:   ${actual_sha512}" >&2
    return 1
  fi

  echo "${DEBIAN_IMAGE_PATH}: OK"
}

download_debian_image() {
  mkdir -p "$DEBIAN_CACHE_DIR"
  if verify_debian_image >/dev/null 2>&1; then
    verify_debian_image
    return
  fi

  echo "Downloading Debian ${DEBIAN_RELEASE} cloud image"
  DOWNLOAD_TEMP=$(mktemp "${DEBIAN_IMAGE_PATH}.part.XXXXXX")
  curl --fail --location \
    --connect-timeout "$DOWNLOAD_CONNECT_TIMEOUT_SECS" \
    --max-time "$DOWNLOAD_TIMEOUT_SECS" \
    --retry 3 \
    --retry-all-errors \
    --retry-max-time "$DOWNLOAD_TIMEOUT_SECS" \
    --output "$DOWNLOAD_TEMP" \
    "${DEBIAN_BASE_URL}/${DEBIAN_IMAGE}"
  mv "$DOWNLOAD_TEMP" "$DEBIAN_IMAGE_PATH"
  DOWNLOAD_TEMP=""
  verify_debian_image
}

prepare_u_boot_source() {
  if [[ ! -d "${UBOOT_CACHE_DIR}/.git" ]]; then
    if [[ -e "$UBOOT_CACHE_DIR" ]]; then
      echo "U-Boot cache path exists but is not a Git repository: ${UBOOT_CACHE_DIR}" >&2
      return 1
    fi
    mkdir -p "$(dirname "$UBOOT_CACHE_DIR")"
    git clone --filter=blob:none --no-checkout "$UBOOT_REPOSITORY" "$UBOOT_CACHE_DIR"
  fi

  if ! git -C "$UBOOT_CACHE_DIR" cat-file -e "${UBOOT_COMMIT}^{commit}" 2>/dev/null; then
    git -C "$UBOOT_CACHE_DIR" fetch --depth=1 origin "$UBOOT_COMMIT"
  fi
  git -C "$UBOOT_CACHE_DIR" checkout --detach --force "$UBOOT_COMMIT"
  test "$(git -C "$UBOOT_CACHE_DIR" rev-parse HEAD)" = "$UBOOT_COMMIT"
}

build_u_boot() {
  local rustsbi_path build_path
  rustsbi_path=$(realpath "$RUSTSBI")
  build_path=$(realpath -m "$UBOOT_BUILD_DIR")
  test -s "$rustsbi_path"
  mkdir -p "$build_path"

  make -C "$UBOOT_CACHE_DIR" O="$build_path" \
    ARCH=riscv CROSS_COMPILE=riscv64-linux-gnu- qemu-riscv64_spl_defconfig
  make -C "$UBOOT_CACHE_DIR" O="$build_path" \
    ARCH=riscv CROSS_COMPILE=riscv64-linux-gnu- OPENSBI="$rustsbi_path" \
    -j"$(nproc)"
  test -s "${build_path}/spl/u-boot-spl"
  test -s "${build_path}/u-boot.itb"
}

stop_qemu() {
  if [[ -n "$QEMU_PID" ]] && kill -0 "$QEMU_PID" 2>/dev/null; then
    kill "$QEMU_PID" 2>/dev/null || true
    wait "$QEMU_PID" 2>/dev/null || true
  fi
}

release_debian_image() {
  if [[ "$DEBIAN_MOUNTED" = true ]]; then
    sudo umount "$DEBIAN_MOUNT_DIR" 2>/dev/null || true
    DEBIAN_MOUNTED=false
  fi
  if [[ -n "$NBD_DEVICE" ]]; then
    sudo qemu-nbd --disconnect "$NBD_DEVICE" 2>/dev/null || true
    NBD_DEVICE=""
  fi
  if [[ -n "$DEBIAN_MOUNT_DIR" ]]; then
    rmdir "$DEBIAN_MOUNT_DIR" 2>/dev/null || true
    DEBIAN_MOUNT_DIR=""
  fi
}

cleanup() {
  if [[ -n "$DOWNLOAD_TEMP" ]]; then
    rm -f "$DOWNLOAD_TEMP"
  fi
  stop_qemu
  if [[ -n "$QEMU_INPUT" ]]; then
    exec 3>&- 3<&- || true
    rm -f "$QEMU_INPUT"
  fi
  release_debian_image
  if [[ -n "$DEBIAN_BOOT_DIR" ]]; then
    rm -rf "$DEBIAN_BOOT_DIR"
  fi
  if [[ -n "$QEMU_OVERLAY" ]]; then
    rm -f "$QEMU_OVERLAY"
  fi
}

create_overlay() {
  local debian_image_path
  debian_image_path=$(realpath "$DEBIAN_IMAGE_PATH")
  mkdir -p "$LOG_DIR"
  : >"$LOG_FILE"
  QEMU_OVERLAY=$(mktemp "${TMPDIR:-/tmp}/rustsbi-debian-overlay.XXXXXX.qcow2")
  qemu-img create -q -f qcow2 -F qcow2 -b "$debian_image_path" "$QEMU_OVERLAY"
}

start_qemu_u_boot() {
  local build_path
  build_path=$(realpath "$UBOOT_BUILD_DIR")
  create_overlay
  QEMU_INPUT=$(mktemp -u "${TMPDIR:-/tmp}/rustsbi-debian-qemu.XXXXXX")
  mkfifo "$QEMU_INPUT"
  exec 3<>"$QEMU_INPUT"

  qemu-system-riscv64 \
    -machine virt \
    -nographic \
    -no-reboot \
    -smp 4 \
    -m 4G \
    -bios "${build_path}/spl/u-boot-spl" \
    -device "loader,file=${build_path}/u-boot.itb,addr=0x80200000" \
    -drive "file=${QEMU_OVERLAY},format=qcow2,if=none,id=hd0" \
    -device virtio-blk-device,drive=hd0 \
    <"$QEMU_INPUT" >"$LOG_FILE" 2>&1 &
  QEMU_PID=$!
}

find_free_nbd() {
  local candidate name sys_device
  for sys_device in /sys/class/block/nbd*; do
    [[ -e "$sys_device" ]] || continue
    name=${sys_device##*/}
    [[ "$name" =~ ^nbd[0-9]+$ ]] || continue
    [[ ! -e "${sys_device}/pid" ]] || continue
    candidate="/dev/${name}"
    if sudo qemu-nbd --format=qcow2 --read-only --connect="$candidate" "$DEBIAN_IMAGE_PATH"; then
      NBD_DEVICE=$candidate
      return 0
    fi
  done

  echo "No free NBD device is available" >&2
  return 1
}

copy_debian_boot_assets() {
  local kernel_magic

  sudo modprobe nbd max_part=16
  sudo udevadm settle
  find_free_nbd
  sudo blockdev --rereadpt "$NBD_DEVICE" 2>/dev/null || true
  sudo partx --add "$NBD_DEVICE" 2>/dev/null || true
  sudo udevadm settle
  for ((tick = 0; tick < 100; tick++)); do
    [[ -b "${NBD_DEVICE}p1" ]] && break
    sleep 0.1
  done
  if [[ ! -b "${NBD_DEVICE}p1" ]]; then
    echo "Debian root partition did not appear on ${NBD_DEVICE}" >&2
    return 1
  fi

  DEBIAN_MOUNT_DIR=$(mktemp -d "${TMPDIR:-/tmp}/rustsbi-debian-mount.XXXXXX")
  sudo mount -o ro "${NBD_DEVICE}p1" "$DEBIAN_MOUNT_DIR"
  DEBIAN_MOUNTED=true

  DEBIAN_BOOT_DIR=$(mktemp -d "${TMPDIR:-/tmp}/rustsbi-debian-boot.XXXXXX")
  sudo cat -- "${DEBIAN_MOUNT_DIR}/boot/vmlinux-${DEBIAN_KERNEL_RELEASE}" >"${DEBIAN_BOOT_DIR}/Image"
  sudo cat -- "${DEBIAN_MOUNT_DIR}/boot/initrd.img-${DEBIAN_KERNEL_RELEASE}" >"${DEBIAN_BOOT_DIR}/initrd.img"

  sudo umount "$DEBIAN_MOUNT_DIR"
  DEBIAN_MOUNTED=false
  sudo qemu-nbd --disconnect "$NBD_DEVICE"
  NBD_DEVICE=""
  rmdir "$DEBIAN_MOUNT_DIR"
  DEBIAN_MOUNT_DIR=""

  test -s "${DEBIAN_BOOT_DIR}/initrd.img"
  kernel_magic=$(dd if="${DEBIAN_BOOT_DIR}/Image" bs=1 skip=48 count=5 status=none)
  if [[ "$kernel_magic" != "RISCV" ]]; then
    echo "Debian kernel does not contain the RISC-V Image magic" >&2
    return 1
  fi
}

start_qemu_sbi() {
  create_overlay
  qemu-system-riscv64 \
    -machine virt \
    -nographic \
    -no-reboot \
    -smp 4 \
    -m 4G \
    -bios "$RUSTSBI" \
    -kernel "${DEBIAN_BOOT_DIR}/Image" \
    -initrd "${DEBIAN_BOOT_DIR}/initrd.img" \
    -append "$KERNEL_OPTIONS" \
    -drive "file=${QEMU_OVERLAY},format=qcow2,if=none,id=hd0" \
    -device virtio-blk-device,drive=hd0 \
    </dev/null >"$LOG_FILE" 2>&1 &
  QEMU_PID=$!
}

qemu_is_running() {
  if kill -0 "$QEMU_PID" 2>/dev/null; then
    return 0
  fi

  local qemu_exit
  set +e
  wait "$QEMU_PID"
  qemu_exit=$?
  set -e
  echo "QEMU exited before Debian reached userspace (exit=${qemu_exit})" >&2
  return 1
}

boot_failed() {
  grep -Eq 'Kernel panic|VFS: Unable to mount root fs|Gave up waiting for root|emergency mode|Bad Linux RISCV Image magic' "$LOG_FILE"
}

userspace_is_ready() {
  grep -Fq "Linux version ${DEBIAN_KERNEL_RELEASE} " "$LOG_FILE" \
    && grep -Fq "EXT4-fs (vda1): mounted filesystem ${DEBIAN_ROOT_UUID}" "$LOG_FILE" \
    && grep -Fq 'Multi-User System' "$LOG_FILE" \
    && grep -Fq 'Debian GNU/Linux 13' "$LOG_FILE" \
    && grep -Eq 'login: *$' "$LOG_FILE"
}

wait_for_debian() {
  local tick command_sent=false autoboot_interrupted=false
  if [[ "$BOOT_MODE" = sbi ]]; then
    command_sent=true
  fi
  for ((tick = 0; tick < BOOT_TIMEOUT_SECS * 10; tick++)); do
    qemu_is_running || return 1
    if boot_failed; then
      echo "Debian reported a fatal boot error" >&2
      return 1
    fi
    if [[ "$BOOT_MODE" = u-boot && "$autoboot_interrupted" = false ]] \
      && grep -Fq 'Hit any key to stop autoboot' "$LOG_FILE"; then
      printf '\n' >&3
      autoboot_interrupted=true
    fi
    if [[ "$BOOT_MODE" = u-boot && "$command_sent" = false ]] && grep -Fq '=>' "$LOG_FILE"; then
      printf '%s\n' "$BOOT_COMMAND" >&3
      command_sent=true
      echo "Sent Debian boot command to U-Boot"
    fi
    if [[ "$command_sent" = true ]] && userspace_is_ready; then
      return 0
    fi
    sleep 0.1
  done

  echo "Debian did not reach userspace within ${BOOT_TIMEOUT_SECS}s" >&2
  return 1
}

main() {
  trap cleanup EXIT
  download_debian_image
  test -s "$RUSTSBI"
  qemu-system-riscv64 --version
  if [[ "$BOOT_MODE" = u-boot ]]; then
    prepare_u_boot_source
    build_u_boot
    start_qemu_u_boot
  else
    copy_debian_boot_assets
    start_qemu_sbi
  fi
  if ! wait_for_debian; then
    tail -n 160 "$LOG_FILE" || true
    return 1
  fi

  echo "RustSBI booted Debian ${DEBIAN_RELEASE} to userspace successfully (${BOOT_MODE})"
  echo "QEMU log: ${LOG_FILE}"
}

main "$@"
