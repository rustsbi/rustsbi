#!/usr/bin/env bash
#
# Boot AOSC OS through RustSBI Prototyper in QEMU.
#
# AOSC OS ships no initramfs: its kernel package only generates one through a
# dpkg trigger at install time, and the pinned release tarball never ran it. The
# script therefore boots the tarball once with a minimal bootstrap initramfs
# that loads the virtio modules, lets the distribution build its own initramfs
# with dracut, and boots again with it. The second boot is what the job asserts
# on, so dracut's early userspace is covered as well.
#
# The direct SBI path hands the kernel to QEMU's fw_dynamic loader. The U-Boot
# path embeds the firmware built from the commit under test as the OpenSBI
# payload of U-Boot SPL and boots the distribution through `sysboot`, the same
# extlinux flow AOSC OS uses on its own U-Boot platforms.
#
# The root disk is assembled from the pinned tarball through a loop mount, and
# the generated initramfs is read back with debugfs, so it needs no mount of its
# own.
#
# Requires: cargo prototyper build, qemu-system-riscv64, curl, xz, zstd,
# e2fsprogs (mkfs.ext4/debugfs), riscv64-linux-gnu-gcc and sudo; the U-Boot
# path additionally needs the U-Boot build toolchain (see aosc-os.yml).
set -euo pipefail

if (( $# != 1 )) || [[ "$1" != sbi && "$1" != u-boot ]]; then
  echo "Usage: $0 <sbi|u-boot>" >&2
  exit 2
fi
readonly BOOT_MODE=$1

cd "$(dirname "$0")/../.."

readonly AOSC_RELEASE=20260621
readonly AOSC_URL="https://releases.aosc.io/os-riscv64/base/aosc-os_base_${AOSC_RELEASE}_riscv64.tar.xz"
readonly AOSC_SHA256=9e8133ba3da5c9951833ad34557fb605987282f3f3df3bd1165eccbe76b39e52
# Kernel packaged in the pinned rootfs; drives the module and initramfs paths.
readonly AOSC_KERNEL_VERSION=7.0.12-aosc-main
readonly AOSC_INITRD_PATH="/boot/initramfs-${AOSC_KERNEL_VERSION}.img"
readonly CROSS_COMPILE=riscv64-linux-gnu-
readonly RUSTSBI_DYNAMIC_BIN=target/riscv64gc-unknown-none-elf/release/rustsbi-prototyper-dynamic.bin
readonly RUSTSBI_FIT_BIN=target/riscv64gc-unknown-none-elf/release/rustsbi-prototyper.bin

readonly UBOOT_VERSION=2024.04
readonly UBOOT_URL="https://github.com/u-boot/u-boot/archive/refs/tags/v${UBOOT_VERSION}.tar.gz"
readonly UBOOT_SHA256=d6b57ce574a0a0504a5b6596644ceacb7f77bde9353779bcf2fde07c4b9a2b92

readonly CACHE_DIR="${AOSC_CACHE_DIR:-.cache/aosc-os}"
readonly ROOTFS_ARCHIVE="${CACHE_DIR}/aosc-os_base_${AOSC_RELEASE}_riscv64.tar.xz"
readonly CACHED_INITRAMFS="${CACHE_DIR}/initramfs-${AOSC_KERNEL_VERSION}.img"
readonly UBOOT_ARCHIVE="${CACHE_DIR}/u-boot-v${UBOOT_VERSION}.tar.gz"
WORK_DIR="$(realpath -m "${AOSC_WORK_DIR:-${RUNNER_TEMP:-/tmp}/rustsbi-aosc-os-${BOOT_MODE}}")"
readonly WORK_DIR
readonly LOG_DIR="${QEMU_LOG_DIR:-qemu-logs}"
readonly LOG_FILE="${LOG_DIR}/prototyper-aosc-os-${BOOT_MODE}.log"

if [[ -n "${AOSC_DISK_IMAGE:-}" ]]; then
  DISK_IMAGE="$(realpath -m "$AOSC_DISK_IMAGE")"
else
  DISK_IMAGE="${WORK_DIR}/aosc-os.img"
fi
readonly DISK_IMAGE
readonly MOUNT_DIR="${WORK_DIR}/mnt"
readonly INITRAMFS_TREE="${WORK_DIR}/bootstrap"
readonly BOOTSTRAP_INITRAMFS="${WORK_DIR}/bootstrap-initramfs.cpio.gz"
readonly KERNEL_IMAGE="${WORK_DIR}/Image"
readonly GENERATED_INITRAMFS="${WORK_DIR}/initramfs-${AOSC_KERNEL_VERSION}.img"
readonly UBOOT_TREE="${WORK_DIR}/u-boot-${UBOOT_VERSION}"
readonly UBOOT_SPL="${UBOOT_TREE}/spl/u-boot-spl"
readonly UBOOT_ITB="${UBOOT_TREE}/u-boot.itb"

readonly DISK_SIZE="${AOSC_DISK_SIZE:-8G}"
readonly GUEST_MEMORY=4G
# Phase 1 covers a full boot plus an in-guest dracut run under emulation.
readonly BOOT_TIMEOUT_SECS="${AOSC_BOOT_TIMEOUT_SECS:-1800}"
readonly DOWNLOAD_CONNECT_TIMEOUT_SECS="${AOSC_DOWNLOAD_CONNECT_TIMEOUT_SECS:-30}"
readonly DOWNLOAD_TIMEOUT_SECS="${AOSC_DOWNLOAD_TIMEOUT_SECS:-1800}"

# The U-Boot path also fails fast when its loader cannot read the extlinux entry
# or the kernel and initrd that entry points at.
readonly BOOT_FAILURE_PATTERN='Kernel panic|not syncing|Attempted to kill init|Bad Linux RISCV Image magic|VFS: Unable to mount root fs|VFS: Cannot open root device|Gave up waiting for root|Error reading config file|for failure retrieving kernel|for failure retrieving initrd'
readonly GENERATE_MARKER="RUSTSBI_AOSC_INITRAMFS_GENERATED ${AOSC_KERNEL_VERSION}"
# One marker per boot path, so an uploaded log says which chain was exercised.
SMOKE_MARKER="RUSTSBI_AOSC_$(tr 'a-z-' 'A-Z_' <<<"$BOOT_MODE")_OK"
readonly SMOKE_MARKER
# Each phase selects exactly one CI unit through the kernel command line, so no
# unit needs a negated condition to stay out of the other phase.
readonly GENERATE_CMDLINE=rustsbi.aosc.generate-initramfs=1
readonly VERIFY_CMDLINE=rustsbi.aosc.verify=1
# dracut's sysroot.mount runs `mount /dev/vda /sysroot -o rw`, which fails
# without a filesystem type, so name it here as the bootstrap init does.
readonly KERNEL_CMDLINE="root=/dev/vda rw rootfstype=ext4"

QEMU_PID=""
DISK_MOUNTED=false

verify_sha256() {
  printf '%s  %s\n' "$1" "$2" | sha256sum --check --status
}

download_asset() {
  local url=$1 destination=$2 digest=$3 temp
  mkdir -p "$(dirname "$destination")"
  if [[ -f "$destination" ]] && verify_sha256 "$digest" "$destination"; then
    echo "Using verified $(basename "$destination")"
    return
  fi
  echo "Downloading ${url}"
  temp=$(mktemp "${destination}.part.XXXXXX")
  curl --fail --location \
    --connect-timeout "$DOWNLOAD_CONNECT_TIMEOUT_SECS" \
    --max-time "$DOWNLOAD_TIMEOUT_SECS" \
    --retry 3 \
    --retry-all-errors \
    --retry-max-time "$DOWNLOAD_TIMEOUT_SECS" \
    --output "$temp" "$url"
  if ! verify_sha256 "$digest" "$temp"; then
    rm -f "$temp"
    echo "SHA-256 mismatch: ${url}" >&2
    return 1
  fi
  mv "$temp" "$destination"
}

# mount_disk [ro]
mount_disk() {
  local mode=${1:-rw}
  mkdir -p "$MOUNT_DIR"
  if [[ "$mode" = ro ]]; then
    sudo mount -o loop,ro "$DISK_IMAGE" "$MOUNT_DIR"
  else
    sudo mount -o loop "$DISK_IMAGE" "$MOUNT_DIR"
  fi
  DISK_MOUNTED=true
}

unmount_disk() {
  [[ "$DISK_MOUNTED" = true ]] || return 0
  sync
  sudo umount "$MOUNT_DIR"
  DISK_MOUNTED=false
}

# Install the two CI-only units into the guest rootfs:
#   * rustsbi-aosc-generate-initramfs.service runs dracut once, in phase 1 only.
#   * rustsbi-aosc-smoke.service asserts on the distribution userspace.
# Both are gated on the kernel command line so the same disk serves both phases.
install_guest_units() {
  local root=$1

  cat > "${WORK_DIR}/generate-initramfs.sh" <<EOF
#!/bin/bash
set -eu
# Same command /usr/bin/update-initramfs runs for this kernel; its trailing
# update-grub step only rewrites bootloader configuration and is skipped.
echo "RUSTSBI_AOSC_GENERATE_BEGIN ${AOSC_KERNEL_VERSION}"
dracut -q --force "${AOSC_INITRD_PATH}" "${AOSC_KERNEL_VERSION}"
test -s "${AOSC_INITRD_PATH}"
sync
echo "${GENERATE_MARKER}"
systemctl --force poweroff
EOF
  chmod 0755 "${WORK_DIR}/generate-initramfs.sh"
  sudo install -D -m 0755 "${WORK_DIR}/generate-initramfs.sh" \
    "${root}/usr/local/sbin/rustsbi-aosc-generate-initramfs"

  cat > "${WORK_DIR}/generate-initramfs.service" <<'EOF'
[Unit]
Description=Generate the AOSC OS initramfs for the RustSBI CI boot test
ConditionKernelCommandLine=rustsbi.aosc.generate-initramfs=1
DefaultDependencies=no
After=local-fs.target
Before=multi-user.target

[Service]
Type=oneshot
ExecStart=/usr/local/sbin/rustsbi-aosc-generate-initramfs
StandardOutput=journal+console
StandardError=journal+console

[Install]
WantedBy=multi-user.target
EOF
  sudo install -D -m 0644 "${WORK_DIR}/generate-initramfs.service" \
    "${root}/etc/systemd/system/rustsbi-aosc-generate-initramfs.service"

  cat > "${WORK_DIR}/smoke.sh" <<EOF
#!/bin/bash
set -euo pipefail
. /etc/os-release
test "\$ID" = aosc
test "\$(uname -m)" = riscv64
test "\$(cat /proc/1/comm)" = systemd
echo "RUSTSBI_AOSC_SMOKE_BEGIN"
uname -a
cat /etc/os-release
echo "${SMOKE_MARKER} \$PRETTY_NAME \$(uname -m)"
EOF
  chmod 0755 "${WORK_DIR}/smoke.sh"
  sudo install -D -m 0755 "${WORK_DIR}/smoke.sh" \
    "${root}/usr/local/sbin/rustsbi-aosc-smoke"

  cat > "${WORK_DIR}/smoke.service" <<'EOF'
[Unit]
Description=RustSBI AOSC OS userspace smoke test
ConditionKernelCommandLine=rustsbi.aosc.verify=1
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
    "${root}/etc/systemd/system/rustsbi-aosc-smoke.service"

  sudo install -d -m 0755 "${root}/etc/systemd/system/multi-user.target.wants"
  sudo ln -sf ../rustsbi-aosc-generate-initramfs.service \
    "${root}/etc/systemd/system/multi-user.target.wants/rustsbi-aosc-generate-initramfs.service"
  sudo ln -sf ../rustsbi-aosc-smoke.service \
    "${root}/etc/systemd/system/multi-user.target.wants/rustsbi-aosc-smoke.service"

  # The distribution default pulls in a two minute wait for the network.
  sudo ln -sf /dev/null "${root}/etc/systemd/system/NetworkManager-wait-online.service"

  # 99-default.preset is "disable *" and preset-all runs on the first boot;
  # preset files match in name order and the first match wins, so this file has
  # to sort before it or the units above get disabled again.
  cat > "${WORK_DIR}/rustsbi-ci.preset" <<'EOF'
enable rustsbi-aosc-generate-initramfs.service
enable rustsbi-aosc-smoke.service
EOF
  sudo install -D -m 0644 "${WORK_DIR}/rustsbi-ci.preset" \
    "${root}/etc/systemd/system-preset/98-rustsbi-ci.preset"
}

# Build the root disk from the pinned tarball and install the CI units.
build_root_disk() {
  truncate -s "$DISK_SIZE" "$DISK_IMAGE"
  mkfs.ext4 -q -F -m 0 -L aosc-os "$DISK_IMAGE"
  mount_disk rw

  sudo tar --same-owner --numeric-owner --xattrs --acls \
    -xJf "$ROOTFS_ARCHIVE" -C "$MOUNT_DIR"

  test -d "${MOUNT_DIR}/usr/lib/modules/${AOSC_KERNEL_VERSION}"
  install_guest_units "$MOUNT_DIR"
  unmount_disk
}

# Pick up the pieces both boot phases need: the kernel image and the virtio
# modules, both read straight out of the root disk.
fetch_boot_artifacts() {
  local module
  mount_disk ro
  cp "${MOUNT_DIR}/usr/lib/aosc-os-riscv64-boot/linux-kernel-${AOSC_KERNEL_VERSION}/Image" \
    "$KERNEL_IMAGE"
  # Bootstrap initramfs: only the virtio transports, expanded to plain .ko
  # because the 7.0.12 kernel has no CONFIG_MODULE_DECOMPRESS and rejects
  # compressed modules with "Invalid ELF header magic".
  rm -rf "$INITRAMFS_TREE"
  install -d -m 0755 "${INITRAMFS_TREE}/lib/modules" "${INITRAMFS_TREE}/proc" \
    "${INITRAMFS_TREE}/sys" "${INITRAMFS_TREE}/dev"
  for module in virtio_pci_modern_dev virtio_pci_legacy_dev virtio_pci virtio_mmio; do
    zstd -d -q -f \
      "${MOUNT_DIR}/usr/lib/modules/${AOSC_KERNEL_VERSION}/kernel/drivers/virtio/${module}.ko.zst" \
      -o "${INITRAMFS_TREE}/lib/modules/${module}.ko"
  done
  unmount_disk

  test -s "$KERNEL_IMAGE"
  local kernel_magic
  kernel_magic=$(dd if="$KERNEL_IMAGE" bs=1 skip=48 count=5 status=none)
  if [[ "$kernel_magic" != RISCV ]]; then
    echo "AOSC OS kernel does not contain the RISC-V Image magic" >&2
    return 1
  fi
}

# AOSC_DISK_IMAGE reuses an already assembled root disk and only refreshes the
# CI units in it; handy when iterating on the boot phases.
adopt_existing_disk() {
  mount_disk rw
  install_guest_units "$MOUNT_DIR"
  unmount_disk
}

# Build U-Boot with the firmware under test embedded as the SPL's OpenSBI
# payload, the same way the other U-Boot boot tests in this repository do.
prepare_uboot() {
  local rustsbi_path tarball
  rustsbi_path=$(realpath "$RUSTSBI_FIT_BIN")
  tarball="${UBOOT_ARCHIVE}"
  download_asset "$UBOOT_URL" "$tarball" "$UBOOT_SHA256"

  rm -rf "$UBOOT_TREE"
  tar -xzf "$tarball" -C "$WORK_DIR"
  make -C "$UBOOT_TREE" ARCH=riscv CROSS_COMPILE="$CROSS_COMPILE" \
    OPENSBI="$rustsbi_path" qemu-riscv64_spl_defconfig
  # U-Boot runs the distribution's own extlinux entry, which is how AOSC OS
  # boots on U-Boot platforms; the distribution initramfs comes from the disk.
  "$UBOOT_TREE/scripts/config" --file "$UBOOT_TREE/.config" --enable USE_BOOTCOMMAND
  "$UBOOT_TREE/scripts/config" --file "$UBOOT_TREE/.config" --set-str BOOTCOMMAND \
    'setenv fdt_high; virtio scan; sysboot virtio 0 any 0x84000000 /boot/extlinux/extlinux.conf'
  make -C "$UBOOT_TREE" ARCH=riscv CROSS_COMPILE="$CROSS_COMPILE" \
    OPENSBI="$rustsbi_path" -j"$(nproc)"

  test -s "$UBOOT_SPL" && test -s "$UBOOT_ITB"
  "$UBOOT_TREE/tools/dumpimage" -T flat_dt -p 1 -o "${WORK_DIR}/rustsbi-from-fit.bin" \
    "$UBOOT_ITB" >"${WORK_DIR}/fit-layout.log"
  cmp "$rustsbi_path" "${WORK_DIR}/rustsbi-from-fit.bin"
  echo "Verified u-boot.itb contains current-commit RustSBI"
}

# Stage the kernel and the distribution initramfs on the root disk, together
# with the extlinux entry U-Boot reads them from.
install_boot_files() {
  mount_disk rw
  sudo install -D -m 0644 "$KERNEL_IMAGE" "${MOUNT_DIR}/boot/Image"
  sudo install -D -m 0644 "$GENERATED_INITRAMFS" "${MOUNT_DIR}/boot/initrd.img"
  cat > "${WORK_DIR}/extlinux.conf" <<EOF
DEFAULT aosc
TIMEOUT 1
LABEL aosc
    KERNEL /boot/Image
    INITRD /boot/initrd.img
    APPEND console=ttyS0,115200 ${KERNEL_CMDLINE} ${VERIFY_CMDLINE}
EOF
  sudo install -D -m 0644 "${WORK_DIR}/extlinux.conf" \
    "${MOUNT_DIR}/boot/extlinux/extlinux.conf"
  unmount_disk
}

# The bootstrap /init loads the virtio modules and hands over to the
# distribution's own systemd. It is the only compiled artifact in this test.
build_bootstrap_initramfs() {
  cat > "${WORK_DIR}/bootstrap-init.c" <<'EOF'
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

  load_module("/lib/modules/virtio_pci_modern_dev.ko");
  load_module("/lib/modules/virtio_pci_legacy_dev.ko");
  load_module("/lib/modules/virtio_pci.ko");
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
    -o "${INITRAMFS_TREE}/init" "${WORK_DIR}/bootstrap-init.c"
  test -s "${INITRAMFS_TREE}/init"

  (
    cd "$INITRAMFS_TREE"
    find . -print0 \
      | cpio --null --create --format=newc --owner=0:0 --quiet \
      | gzip -9
  ) > "$BOOTSTRAP_INITRAMFS"
  test -s "$BOOTSTRAP_INITRAMFS"
}

start_qemu() {
  local initramfs=$1 cmdline=$2
  local cpu_args=()
  # Escape hatch for hosts whose riscv64 cross toolchain targets a newer ISA
  # baseline than rv64gc: the bootstrap /init links a static libc, so it needs a
  # matching QEMU CPU model. CI's ubuntu-24.04 toolchain is rv64gc already.
  [[ -n "${AOSC_QEMU_CPU:-}" ]] && cpu_args=(-cpu "$AOSC_QEMU_CPU")
  : > "$LOG_FILE"
  qemu-system-riscv64 \
    -machine virt \
    -smp 1 \
    -m "$GUEST_MEMORY" \
    -nographic \
    -no-reboot \
    "${cpu_args[@]}" \
    -bios "$RUSTSBI_DYNAMIC_BIN" \
    -kernel "$KERNEL_IMAGE" \
    -initrd "$initramfs" \
    -append "$cmdline" \
    -drive "file=${DISK_IMAGE},format=raw,id=hd0,if=none" \
    -device virtio-blk-device,drive=hd0 \
    </dev/null > "$LOG_FILE" 2>&1 &
  QEMU_PID=$!
}

# The U-Boot path starts from the SPL, which carries the firmware under test,
# and loads u-boot.itb as its next stage; everything after that (kernel and
# initramfs) comes off the root disk through U-Boot's own extlinux flow.
start_qemu_u_boot() {
  local cpu_args=()
  [[ -n "${AOSC_QEMU_CPU:-}" ]] && cpu_args=(-cpu "$AOSC_QEMU_CPU")
  : > "$LOG_FILE"
  qemu-system-riscv64 \
    -machine virt \
    -smp 1 \
    -m "$GUEST_MEMORY" \
    -nographic \
    -no-reboot \
    "${cpu_args[@]}" \
    -bios "$UBOOT_SPL" \
    -device "loader,file=${UBOOT_ITB},addr=0x80200000" \
    -drive "file=${DISK_IMAGE},format=raw,id=hd0,if=none" \
    -device virtio-blk-device,drive=hd0 \
    </dev/null > "$LOG_FILE" 2>&1 &
  QEMU_PID=$!
}

boot_has_failed() {
  grep -Eq "$BOOT_FAILURE_PATTERN|RUSTSBI_AOSC_BOOTSTRAP_FAILED" "$LOG_FILE"
}

wait_for_marker() {
  local marker=$1 description=$2 elapsed qemu_exit
  for ((elapsed = 0; elapsed < BOOT_TIMEOUT_SECS; elapsed++)); do
    if boot_has_failed; then
      echo "AOSC OS ${description} failed" >&2
      grep -E --max-count=5 "$BOOT_FAILURE_PATTERN|RUSTSBI_AOSC_BOOTSTRAP_FAILED" "$LOG_FILE" >&2 || true
      tail -n 120 "$LOG_FILE" >&2
      return 1
    fi
    if grep -Fq "$marker" "$LOG_FILE"; then
      return 0
    fi
    if ! kill -0 "$QEMU_PID" 2>/dev/null; then
      set +e
      wait "$QEMU_PID"
      qemu_exit=$?
      set -e
      QEMU_PID=""
      echo "QEMU exited before AOSC OS ${description} (exit=${qemu_exit})" >&2
      tail -n 120 "$LOG_FILE" >&2
      return 1
    fi
    sleep 1
  done
  echo "AOSC OS ${description} timed out after ${BOOT_TIMEOUT_SECS}s" >&2
  tail -n 120 "$LOG_FILE" >&2
  return 1
}

stop_qemu() {
  if [[ -n "$QEMU_PID" ]] && kill -0 "$QEMU_PID" 2>/dev/null; then
    kill "$QEMU_PID" 2>/dev/null || true
    wait "$QEMU_PID" 2>/dev/null || true
  fi
  QEMU_PID=""
}

cleanup() {
  stop_qemu
  unmount_disk
}

# Phase 1: let the distribution generate its own initramfs.
generate_distribution_initramfs() {
  echo "Phase 1: booting AOSC OS to generate the distribution initramfs"
  start_qemu "$BOOTSTRAP_INITRAMFS" "console=ttyS0,115200 ${KERNEL_CMDLINE} ${GENERATE_CMDLINE}"
  wait_for_marker "$GENERATE_MARKER" "initramfs generation"
  # The unit syncs before printing the marker and then powers off; give QEMU a
  # moment to shut down cleanly so the ext4 image is left consistent.
  local grace
  for ((grace = 0; grace < 60; grace++)); do
    kill -0 "$QEMU_PID" 2>/dev/null || break
    sleep 1
  done
  stop_qemu

  # Read the generated initramfs back off the ext4 image without mounting it.
  rm -f "$GENERATED_INITRAMFS"
  debugfs -R "dump ${AOSC_INITRD_PATH} ${GENERATED_INITRAMFS}" "$DISK_IMAGE" >/dev/null
  test -s "$GENERATED_INITRAMFS"
  cp "$GENERATED_INITRAMFS" "$CACHED_INITRAMFS"
  echo "Distribution initramfs: $(stat -c '%s' "$GENERATED_INITRAMFS") bytes"
}

# Phase 2: boot the pinned AOSC OS with that initramfs.
verify_aosc_os_boot() {
  echo "Phase 2: booting AOSC OS with its own initramfs (${BOOT_MODE})"
  if [[ "$BOOT_MODE" = u-boot ]]; then
    start_qemu_u_boot
  else
    start_qemu "$GENERATED_INITRAMFS" "console=ttyS0,115200 ${KERNEL_CMDLINE} ${VERIFY_CMDLINE}"
  fi
  wait_for_marker "$SMOKE_MARKER" "userspace boot"
  grep -Fq 'Welcome to' "$LOG_FILE"
  if [[ "$BOOT_MODE" = u-boot ]]; then
    grep -Fq "U-Boot ${UBOOT_VERSION}" "$LOG_FILE"
    grep -Fq 'Starting kernel ...' "$LOG_FILE"
  fi
  stop_qemu
}

main() {
  trap cleanup EXIT

  test -s "$RUSTSBI_DYNAMIC_BIN" || {
    echo "Missing ${RUSTSBI_DYNAMIC_BIN}; run 'cargo prototyper build' first" >&2
    return 1
  }
  if [[ "$BOOT_MODE" = u-boot ]]; then
    test -s "$RUSTSBI_FIT_BIN" || {
      echo "Missing ${RUSTSBI_FIT_BIN}; run 'cargo prototyper build' first" >&2
      return 1
    }
  fi
  qemu-system-riscv64 --version
  "${CROSS_COMPILE}gcc" --version | head -n 1

  mkdir -p "$WORK_DIR" "$LOG_DIR" "$CACHE_DIR"
  sudo -n true 2>/dev/null || {
    echo "This script needs passwordless sudo for the loop mount" >&2
    echo "CI runners have it; locally, use a container or a VM as root." >&2
    return 1
  }
  if [[ -n "${AOSC_DISK_IMAGE:-}" ]]; then
    test -s "$DISK_IMAGE" || {
      echo "AOSC_DISK_IMAGE=${DISK_IMAGE} is missing" >&2
      return 1
    }
    echo "Reusing ${DISK_IMAGE}"
    adopt_existing_disk
  else
    download_asset "$AOSC_URL" "$ROOTFS_ARCHIVE" "$AOSC_SHA256"
    build_root_disk
  fi
  fetch_boot_artifacts
  build_bootstrap_initramfs
  # Build U-Boot before the long phase 1 boot, so a toolchain problem fails fast.
  [[ "$BOOT_MODE" = u-boot ]] && prepare_uboot

  if [[ -s "$CACHED_INITRAMFS" ]]; then
    echo "Using cached ${CACHED_INITRAMFS}"
    cp "$CACHED_INITRAMFS" "$GENERATED_INITRAMFS"
  else
    generate_distribution_initramfs
  fi
  [[ "$BOOT_MODE" = u-boot ]] && install_boot_files

  verify_aosc_os_boot

  echo "RustSBI booted AOSC OS ${AOSC_RELEASE} to userspace successfully (${BOOT_MODE})"
  echo "QEMU log: ${LOG_FILE}"
}

main "$@"
