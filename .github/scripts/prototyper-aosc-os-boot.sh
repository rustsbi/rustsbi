#!/usr/bin/env bash
# Boot the AOSC OS container rootfs with its packaged kernel and RustSBI.
# A small initramfs loads virtio and starts AOSC's systemd. All boot paths use
# this same disk and initramfs; U-Boot reads them through extlinux, while EDK II
# receives the kernel and initramfs from QEMU and starts the EFI stub.
set -euo pipefail

# shellcheck source=.github/scripts/ci-edk2.sh
source "$(dirname "${BASH_SOURCE[0]}")/ci-edk2.sh"
# shellcheck source=.github/scripts/ci-uboot.sh
source "$(dirname "${BASH_SOURCE[0]}")/ci-uboot.sh"

if (( $# != 1 )) || [[ "$1" != sbi && "$1" != u-boot && "$1" != edk2 ]]; then
  echo "Usage: $0 <sbi|u-boot|edk2>" >&2
  exit 2
fi
readonly BOOT_MODE=$1
cd "$(dirname "$0")/../.."

readonly AOSC_RELEASE=20260621
readonly AOSC_URL="https://releases.aosc.io/os-riscv64/container/aosc-os_container_${AOSC_RELEASE}_riscv64.tar.xz"
readonly AOSC_SHA256=9eb42f234c6cddcccc6a181df0281796200fdf1b81544bbc1dc08911d34285f7
readonly KERNEL_VERSION=7.1.13-aosc-main
readonly KERNEL_PACKAGE=linux-kernel-7.1.13_7.1.13-1_riscv64.deb
readonly KERNEL_URL="https://repo.aosc.io/debs/pool/stable/main/l/${KERNEL_PACKAGE}"
readonly KERNEL_SHA256=040f01dfe9a420d4797d7b4a445829b3e81c9ef55712c816203756eb252f711e
readonly UBOOT_VERSION=2024.04
readonly CROSS_COMPILE=riscv64-linux-gnu-
readonly RUSTSBI_DYNAMIC_BIN=target/riscv64gc-unknown-none-elf/release/rustsbi-prototyper-dynamic.bin
readonly RUSTSBI_FIT_BIN=target/riscv64gc-unknown-none-elf/release/rustsbi-prototyper.bin

readonly CACHE_DIR="${AOSC_CACHE_DIR:-.cache/aosc-os}"
readonly ROOTFS_ARCHIVE="${CACHE_DIR}/aosc-os_container_${AOSC_RELEASE}_riscv64.tar.xz"
WORK_DIR=$(realpath -m "${AOSC_WORK_DIR:-${RUNNER_TEMP:-/tmp}/rustsbi-aosc-os-${BOOT_MODE}}")
readonly WORK_DIR
readonly DISK_IMAGE="${WORK_DIR}/aosc-os.img"
readonly MOUNT_DIR="${WORK_DIR}/mnt"
readonly INITRAMFS_TREE="${WORK_DIR}/initramfs"
readonly INITRAMFS_IMAGE="${WORK_DIR}/initramfs.cpio.gz"
readonly KERNEL_IMAGE="${WORK_DIR}/Image"
readonly UBOOT_TREE="${WORK_DIR}/u-boot-${UBOOT_VERSION}"
readonly LOG_DIR="${QEMU_LOG_DIR:-qemu-logs}"
readonly LOG_FILE="${LOG_DIR}/prototyper-aosc-os-${BOOT_MODE}.log"
readonly BOOT_TIMEOUT_SECS="${AOSC_BOOT_TIMEOUT_SECS:-600}"
readonly KERNEL_CMDLINE='console=ttyS0,115200 root=/dev/vda rw rootfstype=ext4'
SMOKE_MARKER="RUSTSBI_AOSC_$(tr 'a-z-' 'A-Z_' <<<"$BOOT_MODE")_OK"
readonly SMOKE_MARKER
readonly BOOT_FAILURE_PATTERN='Kernel panic|not syncing|Attempted to kill init|Bad Linux RISCV Image magic|VFS: Unable to mount root fs|VFS: Cannot open root device|Gave up waiting for root|Error reading config file|for failure retrieving kernel|for failure retrieving initrd|RUSTSBI_AOSC_BOOTSTRAP_FAILED'

QEMU_PID=""
DISK_MOUNTED=false

verify_sha256() {
  printf '%s  %s\n' "$1" "$2" | sha256sum --check --status
}

download_asset() {
  local url=$1 destination=$2 digest=$3 temp
  if [[ -f "$destination" ]] && verify_sha256 "$digest" "$destination"; then
    echo "Using verified $(basename "$destination")"
    return
  fi
  echo "Downloading ${url}"
  temp=$(mktemp "${destination}.part.XXXXXX")
  curl --fail --location --connect-timeout 30 --max-time 1800 \
    --retry 3 --retry-all-errors --retry-max-time 1800 \
    --output "$temp" "$url"
  if ! verify_sha256 "$digest" "$temp"; then
    rm -f "$temp"
    echo "SHA-256 mismatch: ${url}" >&2
    return 1
  fi
  mv "$temp" "$destination"
}

unmount_disk() {
  [[ "$DISK_MOUNTED" = true ]] || return 0
  sudo umount "$MOUNT_DIR"
  DISK_MOUNTED=false
}

cleanup() {
  if [[ -n "$QEMU_PID" ]]; then
    kill "$QEMU_PID" 2>/dev/null || true
    wait "$QEMU_PID" 2>/dev/null || true
  fi
  unmount_disk
}

install_smoke_test() {
  local efi_check='test ! -d /sys/firmware/efi'
  if [[ "$BOOT_MODE" = edk2 ]]; then
    efi_check='test -d /sys/firmware/efi'
  fi

  cat > "${WORK_DIR}/smoke.sh" <<EOF
#!/bin/bash
set -euo pipefail
. /etc/os-release
test "\$ID" = aosc
test "\$(uname -m)" = riscv64
test "\$(uname -r)" = "${KERNEL_VERSION}"
test "\$(cat /proc/1/comm)" = systemd
${efi_check}
uname -a
cat /etc/os-release
echo "${SMOKE_MARKER} \$PRETTY_NAME \$(uname -m)"
EOF
  sudo install -D -m 0755 "${WORK_DIR}/smoke.sh" \
    "${MOUNT_DIR}/usr/local/sbin/rustsbi-aosc-smoke"

  cat > "${WORK_DIR}/smoke.service" <<'EOF'
[Unit]
Description=RustSBI AOSC OS userspace smoke test
After=local-fs.target

[Service]
Type=oneshot
ExecStart=/usr/local/sbin/rustsbi-aosc-smoke
StandardOutput=journal+console
StandardError=journal+console

[Install]
WantedBy=multi-user.target
EOF
  sudo install -D -m 0644 "${WORK_DIR}/smoke.service" \
    "${MOUNT_DIR}/etc/systemd/system/rustsbi-aosc-smoke.service"
  sudo install -d "${MOUNT_DIR}/etc/systemd/system/multi-user.target.wants"
  sudo ln -sf ../rustsbi-aosc-smoke.service \
    "${MOUNT_DIR}/etc/systemd/system/multi-user.target.wants/rustsbi-aosc-smoke.service"
  sudo ln -sf /dev/null "${MOUNT_DIR}/etc/systemd/system/NetworkManager-wait-online.service"

  # Keep the test enabled when systemd applies the first-boot presets.
  echo 'enable rustsbi-aosc-smoke.service' > "${WORK_DIR}/rustsbi-ci.preset"
  sudo install -D -m 0644 "${WORK_DIR}/rustsbi-ci.preset" \
    "${MOUNT_DIR}/etc/systemd/system-preset/98-rustsbi-ci.preset"
}

build_initramfs() {
  rm -rf "$INITRAMFS_TREE"
  install -d "${INITRAMFS_TREE}/lib/modules" "${INITRAMFS_TREE}/proc" \
    "${INITRAMFS_TREE}/sys" "${INITRAMFS_TREE}/dev"
  # virtio-blk-device uses the MMIO transport; virtio_blk is built into this kernel.
  zstd -d -q -f \
    "${MOUNT_DIR}/usr/lib/modules/${KERNEL_VERSION}/kernel/drivers/virtio/virtio_mmio.ko.zst" \
    -o "${INITRAMFS_TREE}/lib/modules/virtio_mmio.ko"

  cat > "${WORK_DIR}/init.c" <<'EOF'
// Minimal initramfs init: bring up the virtio transports, mount the AOSC OS
// root filesystem and switch_root into the distribution's systemd.
#include <fcntl.h>
#include <string.h>
#include <sys/mount.h>
#include <sys/stat.h>
#include <sys/syscall.h>
#include <unistd.h>

#define REBOOT_MAGIC1 0xfee1dead
#define REBOOT_MAGIC2 672274793
#define REBOOT_POWER_OFF 0x4321fedc

static void say(const char *s) { (void)!write(1, s, strlen(s)); }

static void power_off(const char *why) {
  say(why);
  (void)!syscall(SYS_reboot, REBOOT_MAGIC1, REBOOT_MAGIC2, REBOOT_POWER_OFF, 0);
  for (;;) pause();
}

static void load_module(const char *path) {
  int fd = open(path, O_RDONLY | O_CLOEXEC);
  if (fd < 0) return;
  (void)!syscall(SYS_finit_module, fd, "", 0);
  close(fd);
}

int main(void) {
  int i;
  mount("proc", "/proc", "proc", 0, 0);
  mount("sysfs", "/sys", "sysfs", 0, 0);
  mount("devtmpfs", "/dev", "devtmpfs", 0, 0);

  load_module("/lib/modules/virtio_mmio.ko");

  for (i = 0; i < 100; i++) {
    if (access("/dev/vda", F_OK) == 0) break;
    usleep(100000);
  }
  if (access("/dev/vda", F_OK) != 0)
    power_off("RUSTSBI_AOSC_BOOTSTRAP_FAILED: root device never appeared\n");

  mkdir("/newroot", 0755);
  if (mount("/dev/vda", "/newroot", "ext4", 0, "") != 0)
    power_off("RUSTSBI_AOSC_BOOTSTRAP_FAILED: cannot mount root filesystem\n");

  mkdir("/newroot/dev", 0755);
  mkdir("/newroot/proc", 0755);
  mkdir("/newroot/sys", 0755);
  mount("/dev", "/newroot/dev", 0, MS_MOVE, 0);
  mount("/proc", "/newroot/proc", 0, MS_MOVE, 0);
  mount("/sys", "/newroot/sys", 0, MS_MOVE, 0);
  if (chdir("/newroot") != 0) power_off("RUSTSBI_AOSC_BOOTSTRAP_FAILED: chdir\n");
  if (chroot("/newroot") != 0) power_off("RUSTSBI_AOSC_BOOTSTRAP_FAILED: chroot\n");

  say("RUSTSBI_AOSC_BOOTSTRAP_HANDOFF\n");
  {
    static char *const argv[] = {"init", 0};
    static char *const envp[] = {"HOME=/", "TERM=linux",
                                 "PATH=/sbin:/bin:/usr/sbin:/usr/bin", 0};
    execve("/usr/lib/systemd/systemd", argv, envp);
  }
  power_off("RUSTSBI_AOSC_BOOTSTRAP_FAILED: cannot start systemd\n");
  return 1;
}
EOF

  "${CROSS_COMPILE}gcc" -static -O2 -march=rv64gc -mabi=lp64d \
    -o "${INITRAMFS_TREE}/init" "${WORK_DIR}/init.c"
  test -s "${INITRAMFS_TREE}/init"

  (
    cd "$INITRAMFS_TREE"
    find . -print0 \
      | cpio --null --create --format=newc --owner=0:0 --quiet \
      | gzip -9
  ) > "$INITRAMFS_IMAGE"
  test -s "$INITRAMFS_IMAGE"
}

prepare_root_disk() {
  truncate -s 8G "$DISK_IMAGE"
  mkfs.ext4 -q -F -m 0 -L aosc-os "$DISK_IMAGE"
  mkdir -p "$MOUNT_DIR"
  sudo mount -o loop "$DISK_IMAGE" "$MOUNT_DIR"
  DISK_MOUNTED=true
  sudo tar --same-owner --numeric-owner --xattrs --acls \
    -xJf "$ROOTFS_ARCHIVE" -C "$MOUNT_DIR"
  sudo dpkg-deb --extract "${CACHE_DIR}/${KERNEL_PACKAGE}" "$MOUNT_DIR"
  install_smoke_test
  cp "${MOUNT_DIR}/usr/lib/aosc-os-riscv64-boot/linux-kernel-${KERNEL_VERSION}/Image" \
    "$KERNEL_IMAGE"
  test "$(dd if="$KERNEL_IMAGE" bs=1 skip=48 count=5 status=none)" = RISCV
  build_initramfs

  if [[ "$BOOT_MODE" = u-boot ]]; then
    sudo install -D -m 0644 "$KERNEL_IMAGE" "${MOUNT_DIR}/boot/Image"
    sudo install -D -m 0644 "$INITRAMFS_IMAGE" "${MOUNT_DIR}/boot/initrd.img"
    cat > "${WORK_DIR}/extlinux.conf" <<EOF
DEFAULT aosc
TIMEOUT 1
LABEL aosc
    KERNEL /boot/Image
    INITRD /boot/initrd.img
    APPEND ${KERNEL_CMDLINE}
EOF
    sudo install -D -m 0644 "${WORK_DIR}/extlinux.conf" \
      "${MOUNT_DIR}/boot/extlinux/extlinux.conf"
  fi
  unmount_disk
}

start_qemu() {
  local boot_args cpu_args=()
  [[ -n "${AOSC_QEMU_CPU:-}" ]] && cpu_args=(-cpu "$AOSC_QEMU_CPU")
  if [[ "$BOOT_MODE" = u-boot ]]; then
    boot_args=(-bios "${UBOOT_TREE}/spl/u-boot-spl"
      -device "loader,file=${UBOOT_TREE}/u-boot.itb,addr=0x80200000")
  elif [[ "$BOOT_MODE" = edk2 ]]; then
    boot_args=(
      -machine "virt,pflash0=pflash0,pflash1=pflash1,acpi=off"
      -bios "$RUSTSBI_DYNAMIC_BIN"
      -blockdev "node-name=pflash0,driver=file,read-only=on,filename=${WORK_DIR}/RISCV_VIRT_CODE.fd"
      -blockdev "node-name=pflash1,driver=file,filename=${WORK_DIR}/RISCV_VIRT_VARS.fd"
      -kernel "$KERNEL_IMAGE"
      -initrd "$INITRAMFS_IMAGE"
      -append "$KERNEL_CMDLINE"
    )
  else
    boot_args=(-bios "$RUSTSBI_DYNAMIC_BIN" -kernel "$KERNEL_IMAGE"
      -initrd "$INITRAMFS_IMAGE" -append "$KERNEL_CMDLINE")
  fi
  qemu-system-riscv64 \
    -machine virt -smp 1 -m 4G -nographic -no-reboot \
    "${cpu_args[@]}" "${boot_args[@]}" \
    -drive "file=${DISK_IMAGE},format=raw,id=hd0,if=none" \
    -device virtio-blk-device,drive=hd0 \
    </dev/null > "$LOG_FILE" 2>&1 &
  QEMU_PID=$!
}

wait_for_boot() {
  local deadline=$((SECONDS + BOOT_TIMEOUT_SECS))
  while (( SECONDS < deadline )); do
    if grep -Eq "$BOOT_FAILURE_PATTERN" "$LOG_FILE"; then
      echo 'AOSC OS boot failed' >&2
      break
    fi
    if grep -Fq "$SMOKE_MARKER" "$LOG_FILE"; then
      grep -Fq 'Welcome to' "$LOG_FILE" || return 1
      if [[ "$BOOT_MODE" = u-boot ]]; then
        grep -Fq "U-Boot ${UBOOT_VERSION}" "$LOG_FILE" || return 1
        grep -Fq 'Starting kernel ...' "$LOG_FILE" || return 1
      elif [[ "$BOOT_MODE" = edk2 ]]; then
        grep -Fq 'RISC-V EDK2 firmware version' "$LOG_FILE" || return 1
      fi
      return 0
    fi
    if ! kill -0 "$QEMU_PID" 2>/dev/null; then
      echo 'QEMU exited before the AOSC OS smoke test completed' >&2
      break
    fi
    sleep 1
  done
  echo "AOSC OS did not boot successfully within ${BOOT_TIMEOUT_SECS}s" >&2
  tail -n 120 "$LOG_FILE" >&2
  return 1
}

main() {
  trap cleanup EXIT
  test -s "$RUSTSBI_DYNAMIC_BIN" || {
    echo "Missing ${RUSTSBI_DYNAMIC_BIN}; run 'cargo prototyper build' first" >&2
    return 1
  }
  sudo -n true
  mkdir -p "$WORK_DIR" "$LOG_DIR" "$CACHE_DIR"
  download_asset "$AOSC_URL" "$ROOTFS_ARCHIVE" "$AOSC_SHA256"
  download_asset "$KERNEL_URL" "${CACHE_DIR}/${KERNEL_PACKAGE}" "$KERNEL_SHA256"
  if [[ "$BOOT_MODE" = u-boot ]]; then
    test -s "$RUSTSBI_FIT_BIN"
    ci_uboot_build "$UBOOT_VERSION" spl aosc-os-spl "$UBOOT_TREE"
    ci_uboot_fit "$UBOOT_TREE" "$RUSTSBI_FIT_BIN"
  elif [[ "$BOOT_MODE" = edk2 ]]; then
    ci_edk2_prepare "$WORK_DIR"
  fi
  prepare_root_disk
  echo "Booting AOSC OS ${AOSC_RELEASE} (${BOOT_MODE})"
  start_qemu
  wait_for_boot
  echo "RustSBI booted AOSC OS ${AOSC_RELEASE} to userspace successfully (${BOOT_MODE})"
  echo "QEMU log: ${LOG_FILE}"
}

main "$@"
