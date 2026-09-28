#!/usr/bin/env bash
# Boot a checksum-pinned Arch Linux RISC-V rootfs through RustSBI and U-Boot/EDK II.
set -euo pipefail

if (( $# != 1 )); then
  echo "Usage: $0 u-boot|edk2" >&2
  exit 2
fi
readonly BOOT_MODE=$1
case "$BOOT_MODE" in
  u-boot | edk2) ;;
  *) echo "Unknown boot mode: $BOOT_MODE" >&2; exit 2 ;;
esac

cd "$(dirname "$0")/../.."
readonly ARCH_VERSION=2026-08-27
readonly ARCH_URL="https://archriscv.felixc.at/images/archriscv-${ARCH_VERSION}.tar.zst"
readonly ARCH_SHA256=a2045c8b62232db2f60d8e4db610dbb5d9e12856dab0ba08634ad3d7cb7ad498
readonly KERNEL_VERSION=6.15.9
readonly KERNEL_URL="https://cdn.kernel.org/pub/linux/kernel/v6.x/linux-${KERNEL_VERSION}.tar.xz"
readonly KERNEL_SHA256=e94f3af85492302f7a819441458f80bca0ad9912e5a4c83c699ff3c63c52957d
readonly UBOOT_VERSION=2024.04
readonly UBOOT_URL="https://github.com/u-boot/u-boot/archive/refs/tags/v${UBOOT_VERSION}.tar.gz"
readonly UBOOT_SHA256=d6b57ce574a0a0504a5b6596644ceacb7f77bde9353779bcf2fde07c4b9a2b92
readonly EDK2_COMMIT=6951dfe7d59d144a3a980bd7eda699db2d8554ac
readonly EDK2_URL=https://github.com/tianocore/edk2.git
readonly CROSS_COMPILE=riscv64-linux-gnu-

readonly CACHE_DIR="${ARCHLINUX_CACHE_DIR:-.cache/archlinux}"
WORK_DIR="${ARCHLINUX_WORK_DIR:-${RUNNER_TEMP:-/tmp}/rustsbi-archlinux-${BOOT_MODE}}"
WORK_DIR="$(realpath -m "$WORK_DIR")"
readonly WORK_DIR
readonly LOG_DIR="${QEMU_LOG_DIR:-qemu-logs}"
readonly LOG_FILE="${LOG_DIR}/prototyper-archlinux-${BOOT_MODE}.log"
readonly ARCH_ARCHIVE="${CACHE_DIR}/archriscv-${ARCH_VERSION}.tar.zst"
readonly KERNEL_IMAGE="${CACHE_DIR}/linux-${KERNEL_VERSION}-Image"
readonly EDK2_CACHE="${CACHE_DIR}/edk2-${EDK2_COMMIT}"
readonly EDK2_CODE="${EDK2_CACHE}/RISCV_VIRT_CODE.fd"
readonly EDK2_VARS_TEMPLATE="${EDK2_CACHE}/RISCV_VIRT_VARS.fd"
readonly DISK_IMAGE="${WORK_DIR}/archlinux.qcow2"
readonly ROOTFS_MOUNT="${WORK_DIR}/mnt"
readonly BOOT_TIMEOUT_SECS="${ARCHLINUX_BOOT_TIMEOUT_SECS:-600}"
readonly DOWNLOAD_TIMEOUT_SECS="${ARCHLINUX_DOWNLOAD_TIMEOUT_SECS:-1800}"
readonly RUSTSBI_DIR=target/riscv64gc-unknown-none-elf/release
readonly RUSTSBI_FIT_BIN="${RUSTSBI_DIR}/rustsbi-prototyper.bin"
readonly RUSTSBI_DYNAMIC_BIN="${RUSTSBI_DIR}/rustsbi-prototyper-dynamic.bin"

QEMU_PID=""
NBD_DEVICE=""
ROOT_MOUNTED=false
ESP_MOUNTED=false
UBOOT_SPL=""
UBOOT_ITB=""
DOWNLOAD_TEMP=""

mkdir -p "$WORK_DIR" "$LOG_DIR" "$CACHE_DIR"
: > "$LOG_FILE"

verify_sha256() {
  printf '%s  %s\n' "$1" "$2" | sha256sum --check --status
}

download_asset() {
  local url=$1 destination=$2 digest=$3
  mkdir -p "$(dirname "$destination")"
  if [[ -f "$destination" ]] && verify_sha256 "$digest" "$destination"; then
    echo "Using verified $(basename "$destination")"
    return
  fi
  echo "Downloading $url"
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

prepare_kernel() {
  local tree="${WORK_DIR}/linux-${KERNEL_VERSION}"
  local tarball="${WORK_DIR}/linux-${KERNEL_VERSION}.tar.xz"
  local option
  if [[ -s "$KERNEL_IMAGE" ]]; then
    echo "Using cached Linux ${KERNEL_VERSION} Image"
    return
  fi
  download_asset "$KERNEL_URL" "$tarball" "$KERNEL_SHA256"
  rm -rf "$tree"
  tar -xJf "$tarball" -C "$WORK_DIR"
  make -C "$tree" ARCH=riscv CROSS_COMPILE="$CROSS_COMPILE" defconfig
  for option in EFI EFI_STUB EFI_PARTITION VIRTIO_BLK VIRTIO_MMIO EXT4_FS DEVTMPFS SERIAL_8250_CONSOLE; do
    if [[ "$("$tree/scripts/config" --file "$tree/.config" --state "$option")" != y ]]; then
      echo "Linux defconfig does not build in CONFIG_${option}" >&2
      return 1
    fi
  done
  make -C "$tree" ARCH=riscv CROSS_COMPILE="$CROSS_COMPILE" -j"$(nproc)" Image
  cp "$tree/arch/riscv/boot/Image" "$KERNEL_IMAGE"
  test -s "$KERNEL_IMAGE"
}

prepare_uboot() {
  local tarball="${WORK_DIR}/u-boot-v${UBOOT_VERSION}.tar.gz"
  local tree="${WORK_DIR}/u-boot-${UBOOT_VERSION}"
  local rustsbi_abs
  download_asset "$UBOOT_URL" "$tarball" "$UBOOT_SHA256"
  rm -rf "$tree"
  tar -xzf "$tarball" -C "$WORK_DIR"
  rustsbi_abs=$(realpath "$RUSTSBI_FIT_BIN")
  make -C "$tree" ARCH=riscv CROSS_COMPILE="$CROSS_COMPILE" \
    OPENSBI="$rustsbi_abs" qemu-riscv64_spl_defconfig
  "$tree/scripts/config" --file "$tree/.config" --enable USE_BOOTCOMMAND
  "$tree/scripts/config" --file "$tree/.config" --set-str BOOTCOMMAND \
    'setenv fdt_high; virtio scan; sysboot virtio 0:2 any 0x84000000 /boot/extlinux/extlinux.conf'
  make -C "$tree" ARCH=riscv CROSS_COMPILE="$CROSS_COMPILE" \
    OPENSBI="$rustsbi_abs" -j"$(nproc)"
  UBOOT_SPL="$tree/spl/u-boot-spl"
  UBOOT_ITB="$tree/u-boot.itb"
  test -s "$UBOOT_SPL" && test -s "$UBOOT_ITB"
  "$tree/tools/dumpimage" -T flat_dt -p 1 -o "$WORK_DIR/rustsbi-from-fit.bin" "$UBOOT_ITB" \
    > "$WORK_DIR/fit-layout.log"
  cmp "$rustsbi_abs" "$WORK_DIR/rustsbi-from-fit.bin"
  echo "Verified u-boot.itb contains current-commit RustSBI"
}

prepare_edk2() {
  local workspace="${WORK_DIR}/edk2-workspace"
  local source="${workspace}/edk2"
  local output="${workspace}/Build/RiscVVirtQemu/RELEASE_GCC5/FV"
  local head
  if [[ $(stat -c %s "$EDK2_CODE" 2>/dev/null || true) == 33554432 &&
        $(stat -c %s "$EDK2_VARS_TEMPLATE" 2>/dev/null || true) == 33554432 ]]; then
    echo "Using cached EDK II $EDK2_COMMIT"
    return
  fi
  rm -rf "$workspace" "$EDK2_CACHE"
  mkdir -p "$workspace" "$EDK2_CACHE"
  git init --quiet "$source"
  git -C "$source" remote add origin "$EDK2_URL"
  git -C "$source" fetch --quiet --depth=1 origin "$EDK2_COMMIT"
  git -C "$source" checkout --quiet --detach FETCH_HEAD
  head=$(git -C "$source" rev-parse HEAD)
  test "$head" = "$EDK2_COMMIT"
  git -C "$source" submodule update --init --depth=1
  (
    cd "$workspace"
    export WORKSPACE="$PWD"
    export PACKAGES_PATH="$source"
    export EDK_TOOLS_PATH="${source}/BaseTools"
    export GCC5_RISCV64_PREFIX="$CROSS_COMPILE"
    set +u
    # shellcheck disable=SC1091
    source "$source/edksetup.sh" --reconfig
    set -u
    make -C "$source/BaseTools" -j"$(nproc)"
    set +u
    # shellcheck disable=SC1091
    source "$source/edksetup.sh" BaseTools
    set -u
    build -a RISCV64 -b RELEASE -p OvmfPkg/RiscVVirt/RiscVVirtQemu.dsc -t GCC5
  )
  cp "$output/RISCV_VIRT_CODE.fd" "$EDK2_CODE"
  cp "$output/RISCV_VIRT_VARS.fd" "$EDK2_VARS_TEMPLATE"
  truncate -s 32M "$EDK2_CODE" "$EDK2_VARS_TEMPLATE"
}

release_disk() {
  if [[ "$ESP_MOUNTED" = true ]]; then
    sudo umount "$ROOTFS_MOUNT/boot" 2>/dev/null || true
    ESP_MOUNTED=false
  fi
  if [[ "$ROOT_MOUNTED" = true ]]; then
    sudo umount "$ROOTFS_MOUNT" 2>/dev/null || true
    ROOT_MOUNTED=false
  fi
  if [[ -n "$NBD_DEVICE" ]]; then
    sudo qemu-nbd -d "$NBD_DEVICE" 2>/dev/null || true
    NBD_DEVICE=""
  fi
}

find_free_nbd() {
  local sys_device name candidate
  sudo modprobe nbd max_part=8
  sudo udevadm settle
  for sys_device in /sys/class/block/nbd*; do
    [[ -e "$sys_device" ]] || continue
    name=${sys_device##*/}
    [[ "$name" =~ ^nbd[0-9]+$ ]] || continue
    [[ ! -e "$sys_device/pid" ]] || continue
    candidate="/dev/$name"
    if sudo qemu-nbd --format=qcow2 --connect="$candidate" "$DISK_IMAGE"; then
      NBD_DEVICE=$candidate
      return
    fi
  done
  echo "No free NBD device" >&2
  return 1
}

prepare_guest_config() {
  local config_dir="${WORK_DIR}/guest-config"
  mkdir -p "$config_dir"
  cat > "$config_dir/rustsbi-arch-smoke" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
{
  echo RUSTSBI_ARCH_SMOKE_BEGIN
  uname -a
  cat /etc/os-release
  test "$(uname -m)" = riscv64
  . /etc/os-release
  test "$ID" = arch
  test "$(cat /proc/1/comm)" = systemd
  if test -r /sys/firmware/efi/fw_platform_size; then
    test "$(cat /sys/firmware/efi/fw_platform_size)" = 64
    echo RUSTSBI_ARCH_EDK2_OK
  else
    echo RUSTSBI_ARCH_UBOOT_OK
  fi
} | tee /var/log/rustsbi-arch-smoke.log /dev/console
EOF
  cat > "$config_dir/rustsbi-arch-smoke.service" <<'EOF'
[Unit]
Description=RustSBI Arch Linux userspace smoke test
After=local-fs.target

[Service]
Type=oneshot
ExecStart=/usr/local/sbin/rustsbi-arch-smoke
StandardOutput=journal+console
StandardError=journal+console

[Install]
WantedBy=multi-user.target
EOF
}

prepare_disk() {
  local partuuid root_spec config_dir="${WORK_DIR}/guest-config"
  download_asset "$ARCH_URL" "$ARCH_ARCHIVE" "$ARCH_SHA256"
  prepare_guest_config
  rm -f "$DISK_IMAGE"
  qemu-img create -q -f qcow2 "$DISK_IMAGE" 3G
  find_free_nbd
  sudo parted -s "$NBD_DEVICE" mklabel gpt
  sudo parted -s "$NBD_DEVICE" mkpart ESP fat32 1MiB 257MiB
  sudo parted -s "$NBD_DEVICE" set 1 esp on
  sudo parted -s "$NBD_DEVICE" mkpart rootfs ext4 257MiB 100%
  sudo partprobe "$NBD_DEVICE"
  sudo udevadm settle
  for ((tick = 0; tick < 100; tick++)); do
    [[ -b "${NBD_DEVICE}p2" ]] && break
    sleep 0.1
  done
  test -b "${NBD_DEVICE}p1" && test -b "${NBD_DEVICE}p2"
  sudo mkfs.fat -F 32 "${NBD_DEVICE}p1"
  sudo mkfs.ext4 -q -F "${NBD_DEVICE}p2"
  partuuid=$(sudo blkid -s PARTUUID -o value "${NBD_DEVICE}p2")
  test -n "$partuuid"
  root_spec="PARTUUID=$partuuid"
  echo "Arch root: $root_spec"
  mkdir -p "$ROOTFS_MOUNT"
  sudo mount "${NBD_DEVICE}p2" "$ROOTFS_MOUNT"
  ROOT_MOUNTED=true
  # GNU tar ignores archived BSD file flags unsupported by Linux ext4.
  sudo tar --zstd --xattrs --acls --numeric-owner -xf "$ARCH_ARCHIVE" -C "$ROOTFS_MOUNT"
  sudo install -D -m 755 "$config_dir/rustsbi-arch-smoke" \
    "$ROOTFS_MOUNT/usr/local/sbin/rustsbi-arch-smoke"
  sudo install -D -m 644 "$config_dir/rustsbi-arch-smoke.service" \
    "$ROOTFS_MOUNT/etc/systemd/system/rustsbi-arch-smoke.service"
  sudo mkdir -p "$ROOTFS_MOUNT/etc/systemd/system/multi-user.target.wants" \
    "$ROOTFS_MOUNT/boot/extlinux"
  sudo ln -s ../rustsbi-arch-smoke.service \
    "$ROOTFS_MOUNT/etc/systemd/system/multi-user.target.wants/rustsbi-arch-smoke.service"
  sudo cp "$KERNEL_IMAGE" "$ROOTFS_MOUNT/boot/Image"
  cat > "$config_dir/extlinux.conf" <<EOF
DEFAULT arch
TIMEOUT 1
LABEL arch
    KERNEL /boot/Image
    APPEND root=$root_spec rootwait rw console=ttyS0,115200
EOF
  sudo install -m 644 "$config_dir/extlinux.conf" "$ROOTFS_MOUNT/boot/extlinux/extlinux.conf"
  sudo mount "${NBD_DEVICE}p1" "$ROOTFS_MOUNT/boot"
  ESP_MOUNTED=true
  sudo cp "$KERNEL_IMAGE" "$ROOTFS_MOUNT/boot/linux-${KERNEL_VERSION}.elf"
  printf '\\linux-%s.elf rw root=%s rootwait console=ttyS0,115200\r\n' \
    "$KERNEL_VERSION" "$root_spec" > "$config_dir/startup.nsh"
  sudo install -m 644 "$config_dir/startup.nsh" "$ROOTFS_MOUNT/boot/startup.nsh"
  sync
  release_disk
}

start_qemu() {
  : > "$LOG_FILE"
  if [[ "$BOOT_MODE" = u-boot ]]; then
    qemu-system-riscv64 -machine virt -smp 1 -m 4G -nographic -no-reboot \
      -bios "$UBOOT_SPL" -device "loader,file=${UBOOT_ITB},addr=0x80200000" \
      -drive "file=${DISK_IMAGE},format=qcow2,id=hd0,if=none" \
      -device virtio-blk-device,drive=hd0 \
      </dev/null > "$LOG_FILE" 2>&1 &
  else
    cp "$EDK2_VARS_TEMPLATE" "$WORK_DIR/RISCV_VIRT_VARS.fd"
    qemu-system-riscv64 -machine virt,pflash0=pflash0,pflash1=pflash1,acpi=off \
      -smp 1 -m 4G -nographic -no-reboot -bios "$RUSTSBI_DYNAMIC_BIN" \
      -blockdev "node-name=pflash0,driver=file,read-only=on,filename=${EDK2_CODE}" \
      -blockdev "node-name=pflash1,driver=file,filename=${WORK_DIR}/RISCV_VIRT_VARS.fd" \
      -drive "file=${DISK_IMAGE},format=qcow2,id=hd0,if=none" \
      -device virtio-blk-device,drive=hd0 \
      </dev/null > "$LOG_FILE" 2>&1 &
  fi
  QEMU_PID=$!
}

boot_failed() {
  grep -Eq 'Kernel panic|VFS: Unable to mount root fs|Attempted to kill init' "$LOG_FILE"
}

userspace_ready() {
  local marker
  if [[ "$BOOT_MODE" = u-boot ]]; then
    marker=RUSTSBI_ARCH_UBOOT_OK
    grep -Fq 'Starting kernel ...' "$LOG_FILE" || return 1
  else
    marker=RUSTSBI_ARCH_EDK2_OK
    grep -Fq 'EFI stub: Booting Linux Kernel' "$LOG_FILE" || return 1
    grep -Fq 'efi: EFI v2.7 by EDK II' "$LOG_FILE" || return 1
  fi
  grep -Fq '[RustSBI]' "$LOG_FILE" \
    && grep -Fq 'VFS: Mounted root (ext4 filesystem)' "$LOG_FILE" \
    && grep -Fq 'Linux archlinux ' "$LOG_FILE" \
    && grep -Fq 'NAME="Arch Linux"' "$LOG_FILE" \
    && grep -Fq "$marker" "$LOG_FILE"
}

wait_for_userspace() {
  local elapsed qemu_exit
  for ((elapsed = 0; elapsed < BOOT_TIMEOUT_SECS; elapsed++)); do
    if boot_failed; then
      echo "Arch boot failed: kernel panic" >&2
      tail -n 100 "$LOG_FILE" >&2
      return 1
    fi
    if userspace_ready; then
      return 0
    fi
    if ! kill -0 "$QEMU_PID" 2>/dev/null; then
      set +e
      wait "$QEMU_PID"
      qemu_exit=$?
      set -e
      QEMU_PID=""
      echo "QEMU exited before Arch userspace (exit=$qemu_exit)" >&2
      tail -n 100 "$LOG_FILE" >&2
      return 1
    fi
    sleep 1
  done
  echo "Arch userspace not reached within ${BOOT_TIMEOUT_SECS}s" >&2
  tail -n 100 "$LOG_FILE" >&2
  return 1
}

cleanup() {
  if [[ -n "$QEMU_PID" ]] && kill -0 "$QEMU_PID" 2>/dev/null; then
    kill "$QEMU_PID" 2>/dev/null || true
    wait "$QEMU_PID" 2>/dev/null || true
  fi
  release_disk
  if [[ -n "$DOWNLOAD_TEMP" ]]; then
    rm -f "$DOWNLOAD_TEMP"
  fi
}

main() {
  trap cleanup EXIT
  qemu-system-riscv64 --version
  "$CROSS_COMPILE"gcc --version | head -n 1
  test -s "$RUSTSBI_FIT_BIN" && test -s "$RUSTSBI_DYNAMIC_BIN"
  prepare_kernel
  prepare_disk
  if [[ "$BOOT_MODE" = u-boot ]]; then
    prepare_uboot
  else
    prepare_edk2
  fi
  start_qemu
  wait_for_userspace
  echo "RustSBI -> $BOOT_MODE -> Linux -> Arch userspace: PASS"
  echo "Serial log: $LOG_FILE"
}

main "$@"
