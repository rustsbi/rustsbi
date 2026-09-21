#!/usr/bin/env bash
set -euo pipefail

readonly FEDORA_RELEASE="44"
readonly FEDORA_IMAGE="Fedora-Cloud-Base-Generic-44-20260604.0.riscv64.qcow2"
readonly FEDORA_BASE_URL="https://dl.fedoraproject.org/pub/alt/risc-v/release/44/Cloud/riscv64/images"
readonly FEDORA_SHA256="06852158be651467e3a696ce37cbade2dddb067d425689f4b5d6433b9589c27a"
readonly FEDORA_ROOT_UUID="3ffbbe68-1f01-49a4-8454-bf0ce0d84f8f"
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
    echo "Unknown Fedora boot mode: ${BOOT_MODE}" >&2
    echo "Usage: $0 [sbi|u-boot]" >&2
    exit 2
    ;;
esac

readonly FEDORA_CACHE_DIR="${FEDORA_CACHE_DIR:-.cache/fedora/${FEDORA_RELEASE}}"
readonly FEDORA_IMAGE_PATH="${FEDORA_CACHE_DIR}/${FEDORA_IMAGE}"
readonly UBOOT_CACHE_DIR="${UBOOT_CACHE_DIR:-.cache/u-boot/${UBOOT_VERSION}}"
readonly UBOOT_BUILD_DIR="${UBOOT_BUILD_DIR:-target/u-boot-fedora}"
readonly RUSTSBI="target/riscv64gc-unknown-none-elf/release/rustsbi-prototyper-dynamic.bin"
readonly LOG_DIR="${QEMU_LOG_DIR:-qemu-logs}"
readonly LOG_FILE="${LOG_DIR}/prototyper-fedora-${BOOT_MODE}.log"
readonly BOOT_TIMEOUT_SECS="${FEDORA_BOOT_TIMEOUT_SECS:-600}"
readonly DOWNLOAD_CONNECT_TIMEOUT_SECS="${FEDORA_DOWNLOAD_CONNECT_TIMEOUT_SECS:-30}"
readonly DOWNLOAD_TIMEOUT_SECS="${FEDORA_DOWNLOAD_TIMEOUT_SECS:-1800}"
readonly BOOT_COMMAND='fatload virtio 0:1 84000000 EFI/BOOT/BOOTRISCV64.EFI; bootefi 0x84000000 ${fdtcontroladdr}'

QEMU_PID=""
DOWNLOAD_TEMP=""
QEMU_INPUT=""
QEMU_OVERLAY=""
NBD_DEVICE=""
FEDORA_MOUNT_DIR=""
FEDORA_MOUNTED=false
FEDORA_BOOT_DIR=""
FEDORA_KERNEL=""
FEDORA_INITRD=""
FEDORA_KERNEL_OPTIONS=""

verify_fedora_image() {
  local actual_sha256
  [[ -f "$FEDORA_IMAGE_PATH" ]] || return 1
  actual_sha256=$(sha256sum "$FEDORA_IMAGE_PATH")
  actual_sha256=${actual_sha256%% *}

  if [[ "$actual_sha256" != "$FEDORA_SHA256" ]]; then
    echo "Fedora image checksum mismatch" >&2
    echo "expected: ${FEDORA_SHA256}" >&2
    echo "actual:   ${actual_sha256}" >&2
    return 1
  fi

  echo "${FEDORA_IMAGE_PATH}: OK"
}

download_fedora_image() {
  mkdir -p "$FEDORA_CACHE_DIR"
  if verify_fedora_image >/dev/null 2>&1; then
    verify_fedora_image
    return
  fi

  echo "Downloading Fedora ${FEDORA_RELEASE} cloud image"
  DOWNLOAD_TEMP=$(mktemp "${FEDORA_IMAGE_PATH}.part.XXXXXX")
  curl --fail --location \
    --connect-timeout "$DOWNLOAD_CONNECT_TIMEOUT_SECS" \
    --max-time "$DOWNLOAD_TIMEOUT_SECS" \
    --retry 3 \
    --retry-all-errors \
    --retry-max-time "$DOWNLOAD_TIMEOUT_SECS" \
    --output "$DOWNLOAD_TEMP" \
    "${FEDORA_BASE_URL}/${FEDORA_IMAGE}"
  mv "$DOWNLOAD_TEMP" "$FEDORA_IMAGE_PATH"
  DOWNLOAD_TEMP=""
  verify_fedora_image
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

release_fedora_image() {
  if [[ "$FEDORA_MOUNTED" = true ]]; then
    sudo umount "$FEDORA_MOUNT_DIR" 2>/dev/null || true
    FEDORA_MOUNTED=false
  fi
  if [[ -n "$NBD_DEVICE" ]]; then
    sudo qemu-nbd --disconnect "$NBD_DEVICE" 2>/dev/null || true
    NBD_DEVICE=""
  fi
  if [[ -n "$FEDORA_MOUNT_DIR" ]]; then
    rmdir "$FEDORA_MOUNT_DIR" 2>/dev/null || true
    FEDORA_MOUNT_DIR=""
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
  release_fedora_image
  if [[ -n "$FEDORA_BOOT_DIR" ]]; then
    rm -rf "$FEDORA_BOOT_DIR"
  fi
  if [[ -n "$QEMU_OVERLAY" ]]; then
    rm -f "$QEMU_OVERLAY"
  fi
}

create_overlay() {
  local fedora_image_path
  fedora_image_path=$(realpath "$FEDORA_IMAGE_PATH")
  mkdir -p "$LOG_DIR"
  : >"$LOG_FILE"
  QEMU_OVERLAY=$(mktemp "${TMPDIR:-/tmp}/rustsbi-fedora-overlay.XXXXXX.qcow2")
  qemu-img create -q -f qcow2 -F qcow2 -b "$fedora_image_path" "$QEMU_OVERLAY"
}

start_qemu_u_boot() {
  local build_path
  build_path=$(realpath "$UBOOT_BUILD_DIR")
  create_overlay
  QEMU_INPUT=$(mktemp -u "${TMPDIR:-/tmp}/rustsbi-fedora-qemu.XXXXXX")
  mkfifo "$QEMU_INPUT"
  exec 3<>"$QEMU_INPUT"

  qemu-system-riscv64 \
    -machine virt \
    -nographic \
    -no-reboot \
    -smp 4 \
    -m 8G \
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
    if sudo qemu-nbd --format=qcow2 --read-only --connect="$candidate" "$FEDORA_IMAGE_PATH"; then
      NBD_DEVICE=$candidate
      return 0
    fi
  done

  echo "No free NBD device is available" >&2
  return 1
}

copy_fedora_boot_assets() {
  local bls_dir bls_entry initrd_path linux_path
  local -a bls_entries

  sudo modprobe nbd max_part=16
  sudo udevadm settle
  find_free_nbd
  sudo blockdev --rereadpt "$NBD_DEVICE" 2>/dev/null || true
  sudo partx --add "$NBD_DEVICE" 2>/dev/null || true
  sudo udevadm settle
  for ((tick = 0; tick < 100; tick++)); do
    [[ -b "${NBD_DEVICE}p2" ]] && break
    sleep 0.1
  done
  if [[ ! -b "${NBD_DEVICE}p2" ]]; then
    echo "Fedora partition did not appear on ${NBD_DEVICE}" >&2
    return 1
  fi

  FEDORA_MOUNT_DIR=$(mktemp -d "${TMPDIR:-/tmp}/rustsbi-fedora-mount.XXXXXX")
  sudo mount -o ro,subvolid=5 "${NBD_DEVICE}p2" "$FEDORA_MOUNT_DIR"
  FEDORA_MOUNTED=true

  bls_dir="${FEDORA_MOUNT_DIR}/boot/loader/entries"
  mapfile -t bls_entries < <(sudo find "$bls_dir" -maxdepth 1 -type f -name '*.conf' -print)
  if (( ${#bls_entries[@]} != 1 )); then
    echo "Expected exactly one Fedora BLS entry, found ${#bls_entries[@]}" >&2
    return 1
  fi
  bls_entry=${bls_entries[0]}
  linux_path=$(sudo awk '$1 == "linux" { $1 = ""; sub(/^ /, ""); print; exit }' "$bls_entry")
  initrd_path=$(sudo awk '$1 == "initrd" { $1 = ""; sub(/^ /, ""); print; exit }' "$bls_entry")
  FEDORA_KERNEL_OPTIONS=$(sudo awk '$1 == "options" { $1 = ""; sub(/^ /, ""); print; exit }' "$bls_entry")

  if [[ "$linux_path" != /boot/* || "$linux_path" == *..* ]]; then
    echo "Invalid Fedora kernel path in BLS entry: ${linux_path}" >&2
    return 1
  fi
  if [[ "$initrd_path" != /boot/* || "$initrd_path" == *..* ]]; then
    echo "Invalid Fedora initramfs path in BLS entry: ${initrd_path}" >&2
    return 1
  fi
  if [[ " ${FEDORA_KERNEL_OPTIONS} " != *" root=UUID=${FEDORA_ROOT_UUID} "* ]]; then
    echo "Fedora BLS options do not contain the expected root UUID" >&2
    return 1
  fi
  if [[ " ${FEDORA_KERNEL_OPTIONS} " != *" rootflags=subvol=root "* ]]; then
    echo "Fedora BLS options do not contain rootflags=subvol=root" >&2
    return 1
  fi

  FEDORA_BOOT_DIR=$(mktemp -d "${TMPDIR:-/tmp}/rustsbi-fedora-boot.XXXXXX")
  sudo cat -- "${FEDORA_MOUNT_DIR}${linux_path}" >"${FEDORA_BOOT_DIR}/vmlinuz"
  sudo cat -- "${FEDORA_MOUNT_DIR}${initrd_path}" >"${FEDORA_BOOT_DIR}/initramfs.img"
  FEDORA_INITRD="${FEDORA_BOOT_DIR}/initramfs.img"

  sudo umount "$FEDORA_MOUNT_DIR"
  FEDORA_MOUNTED=false
  sudo qemu-nbd --disconnect "$NBD_DEVICE"
  NBD_DEVICE=""
  rmdir "$FEDORA_MOUNT_DIR"
  FEDORA_MOUNT_DIR=""
}

extract_fedora_kernel() {
  local compression image_type kernel_magic payload_offset payload_size zboot_size
  image_type=$(dd if="${FEDORA_BOOT_DIR}/vmlinuz" bs=1 skip=4 count=4 status=none)
  compression=$(dd if="${FEDORA_BOOT_DIR}/vmlinuz" bs=1 skip=24 count=4 status=none)
  if [[ "$image_type" != "zimg" || "$compression" != "zstd" ]]; then
    echo "Unsupported Fedora zboot format: image=${image_type}, compression=${compression}" >&2
    return 1
  fi

  read -r payload_offset < <(od -An -tu4 -j 8 -N 4 "${FEDORA_BOOT_DIR}/vmlinuz")
  read -r payload_size < <(od -An -tu4 -j 12 -N 4 "${FEDORA_BOOT_DIR}/vmlinuz")
  zboot_size=$(stat -c %s "${FEDORA_BOOT_DIR}/vmlinuz")
  if (( payload_offset <= 0 || payload_size <= 0 || payload_offset + payload_size > zboot_size )); then
    echo "Invalid Fedora zboot payload range: offset=${payload_offset}, size=${payload_size}" >&2
    return 1
  fi

  FEDORA_KERNEL="${FEDORA_BOOT_DIR}/Image"
  dd if="${FEDORA_BOOT_DIR}/vmlinuz" bs=1 skip="$payload_offset" count="$payload_size" status=none \
    | zstd --decompress --stdout >"$FEDORA_KERNEL"
  test -s "$FEDORA_KERNEL"
  kernel_magic=$(dd if="$FEDORA_KERNEL" bs=1 skip=48 count=5 status=none)
  if [[ "$kernel_magic" != "RISCV" ]]; then
    echo "Decompressed Fedora kernel does not contain the RISC-V Image magic" >&2
    return 1
  fi
}

prepare_sbi_boot() {
  copy_fedora_boot_assets
  extract_fedora_kernel
}

start_qemu_sbi() {
  create_overlay
  qemu-system-riscv64 \
    -machine virt \
    -nographic \
    -no-reboot \
    -smp 4 \
    -m 8G \
    -bios "$RUSTSBI" \
    -kernel "$FEDORA_KERNEL" \
    -initrd "$FEDORA_INITRD" \
    -append "$FEDORA_KERNEL_OPTIONS" \
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
  echo "QEMU exited before Fedora reached userspace (exit=${qemu_exit})" >&2
  return 1
}

boot_failed() {
  grep -Eq 'Kernel panic|VFS: Unable to mount root fs|No bootable device' "$LOG_FILE"
}

userspace_is_ready() {
  grep -Fq 'Linux version ' "$LOG_FILE" \
    && grep -Fq "BTRFS info (device vda2): first mount of filesystem ${FEDORA_ROOT_UUID}" "$LOG_FILE" \
    && grep -Fq 'multi-user.target' "$LOG_FILE" \
    && grep -Fq 'Multi-User System' "$LOG_FILE" \
    && grep -Fq 'Fedora Linux 44 (Cloud Edition)' "$LOG_FILE" \
    && grep -Fq 'localhost login:' "$LOG_FILE"
}

wait_for_fedora() {
  local tick command_sent=false autoboot_interrupted=false
  if [[ "$BOOT_MODE" = sbi ]]; then
    command_sent=true
  fi
  for ((tick = 0; tick < BOOT_TIMEOUT_SECS * 10; tick++)); do
    qemu_is_running || return 1
    if boot_failed; then
      echo "Fedora reported a fatal boot error" >&2
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
      echo "Sent Fedora EFI boot command to U-Boot"
    fi
    if [[ "$command_sent" = true ]] && userspace_is_ready; then
      return 0
    fi
    sleep 0.1
  done

  echo "Fedora did not reach userspace within ${BOOT_TIMEOUT_SECS}s" >&2
  return 1
}

main() {
  trap cleanup EXIT
  download_fedora_image
  test -s "$RUSTSBI"
  qemu-system-riscv64 --version
  if [[ "$BOOT_MODE" = u-boot ]]; then
    prepare_u_boot_source
    build_u_boot
    start_qemu_u_boot
  else
    prepare_sbi_boot
    start_qemu_sbi
  fi
  if ! wait_for_fedora; then
    tail -n 160 "$LOG_FILE" || true
    return 1
  fi

  echo "RustSBI booted Fedora ${FEDORA_RELEASE} to userspace successfully (${BOOT_MODE})"
  echo "QEMU log: ${LOG_FILE}"
}

main "$@"
