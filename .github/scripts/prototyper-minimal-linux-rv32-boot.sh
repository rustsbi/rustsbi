#!/usr/bin/env bash
#
# Boot a minimal RV32 Linux userspace through RustSBI Prototyper in QEMU.
#
# This is the RV32 counterpart of prototyper-minimal-linux-boot.sh: the
# firmware is built for `riscv32imac-unknown-none-elf` and booted on
# qemu-system-riscv32's `virt` machine.
#
# The direct boot path (`$0 sbi`, the default) follows the fw_dynamic
# handoff convention like the RV64 job: `-bios` points at the RustSBI
# dynamic firmware ELF and `-kernel` at the flattened RV32 kernel image,
# with QEMU handing the kernel entry point to the firmware through the
# dynamic info structure.
#
# A U-Boot-based boot path is selected with `$0 u-boot`: the SPL embeds
# RustSBI Prototyper as its OpenSBI payload, so the chain is QEMU ->
# u-boot-spl -> RustSBI -> u-boot.itb (S-mode) -> Linux. U-Boot loads the
# kernel off an ext4 partition with `ext4load` and starts it with `booti`.
# EDK II has no RV64-free counterpart here and is out of scope for this
# job.
#
# Ubuntu ships no riscv32 Linux cross toolchain, so the pinned Bootlin
# riscv32-ilp32d glibc toolchain (GCC 13.3) is downloaded with its
# published sha256. GCC 13 matters: like the RV64 job, newer compilers
# default to extension sets the QEMU virt CPU rejects.
#
# Linux and BusyBox are built from the same checksum-pinned release
# archives as the RV64 job. Only build *products* are kept under
# CACHE_DIR, so the workflow restores a small cache and skips compilation
# entirely on a hit; the archives themselves are verified on every
# download. RustSBI is rebuilt from the commit under test and is never
# cached. The U-Boot build embeds the RustSBI firmware, so it is never
# cached either.
#
# Requires: `cargo prototyper build --target riscv32imac-unknown-none-elf`
# to have produced the firmware, plus qemu-system-riscv32, cpio, xz,
# parted, e2fsprogs, qemu-utils, swig (< 4.3) and a host toolchain.

set -euo pipefail

if (( $# > 1 )); then
  echo "Usage: $0 [sbi|u-boot]" >&2
  exit 2
fi

readonly BOOT_MODE="${1:-sbi}"
case "$BOOT_MODE" in
  sbi | u-boot) ;;
  *)
    echo "Unknown boot mode: ${BOOT_MODE}" >&2
    echo "Usage: $0 [sbi|u-boot]" >&2
    exit 2
    ;;
esac

readonly KERNEL_VERSION="6.12.110"
readonly KERNEL_URL="https://cdn.kernel.org/pub/linux/kernel/v6.x/linux-${KERNEL_VERSION}.tar.xz"
readonly KERNEL_SHA256="8cee19e1839bb6ff4d5254d761933ae6ab670492d5ed030e09a80538320d5c4c"

readonly BUSYBOX_VERSION="1.36.1"
readonly BUSYBOX_URL="https://busybox.net/downloads/busybox-${BUSYBOX_VERSION}.tar.bz2"
readonly BUSYBOX_SHA256="b8cc24c9574d809e7279c3be349795c5d5ceb6fdf19ca709f80cde50e47de314"

readonly UB_VERSION="2024.04"
readonly UB_URL="https://github.com/u-boot/u-boot/archive/refs/tags/v${UB_VERSION}.tar.gz"
readonly UB_SHA256="d6b57ce574a0a0504a5b6596644ceacb7f77bde9353779bcf2fde07c4b9a2b92"

# Bootlin prebuilt riscv32-ilp32d glibc toolchain; the sha256 is the one
# published next to the tarball on toolchains.bootlin.com. GCC 13.3 is new
# enough for Linux 6.12 and old enough not to emit Zba/Zbb by default.
readonly TOOLCHAIN_RELEASE="riscv32-ilp32d--glibc--stable-2024.05-1"
readonly TOOLCHAIN_URL="https://toolchains.bootlin.com/downloads/releases/toolchains/riscv32-ilp32d/tarballs/${TOOLCHAIN_RELEASE}.tar.xz"
readonly TOOLCHAIN_SHA256="00112418e6d4b0733019a673b682a39f1ce6300b9448cd840f1194aa4b064192"

readonly CROSS_PREFIX="riscv32-buildroot-linux-gnu-"
readonly SMOKE_MARKER="RUSTSBI-SMOKE-OK"

# Text that means the boot already went wrong. A panic halts the machine while
# QEMU stays alive, so without this the job would sit out the whole timeout and
# then report only that the marker never appeared.
readonly BOOT_FAILURE_PATTERN="Kernel panic|not syncing|Attempted to kill init"

readonly CACHE_DIR="${MINIMAL_LINUX_RV32_CACHE_DIR:-.cache/minimal-linux-rv32}"
readonly WORK_DIR="${MINIMAL_LINUX_RV32_WORK_DIR:-.minimal-linux-rv32/work}"
readonly LOG_DIR="${QEMU_LOG_DIR:-qemu-logs}"
readonly LOG_FILE="${LOG_DIR}/prototyper-minimal-linux-rv32-${BOOT_MODE}.log"

# The sbi path boots the dynamic firmware ELF directly; the u-boot path embeds
# the flattened binary in the SPL. The bootloader path gets more time.
if [[ "$BOOT_MODE" = u-boot ]]; then
  readonly RUSTSBI="${MINIMAL_LINUX_RUSTSBI_BIN:-target/riscv32imac-unknown-none-elf/release/rustsbi-prototyper.bin}"
  readonly BOOT_TIMEOUT_SECS="${MINIMAL_LINUX_BOOT_TIMEOUT_SECS:-240}"
else
  readonly RUSTSBI="${MINIMAL_LINUX_RUSTSBI:-target/riscv32imac-unknown-none-elf/release/rustsbi-prototyper-dynamic.elf}"
  readonly BOOT_TIMEOUT_SECS="${MINIMAL_LINUX_BOOT_TIMEOUT_SECS:-180}"
fi

# Absolutized so the exported toolchain bin/ stays on PATH for make
# subshells that run with a different working directory (the kernel tree).
# Done lexically ($PWD prefix) rather than with readlink -f: nothing has
# created the cache directories yet when this runs, and readlink -f needs
# every parent directory to exist.
toolchain_dir="${RV32_TOOLCHAIN_DIR:-${CACHE_DIR}/${TOOLCHAIN_RELEASE}}"
case "$toolchain_dir" in
  /*) ;;
  *) toolchain_dir="$PWD/$toolchain_dir" ;;
esac
readonly TOOLCHAIN_DIR="$toolchain_dir"
readonly KERNEL_IMAGE="${CACHE_DIR}/linux-${KERNEL_VERSION}-Image"
readonly BUSYBOX_INSTALL="${CACHE_DIR}/busybox-${BUSYBOX_VERSION}-install"
readonly INITRAMFS="${WORK_DIR}/initramfs.cpio.gz"
readonly DISK_IMAGE="${WORK_DIR}/linux-rootfs.img"
readonly DISK_PARTITION="${WORK_DIR}/linux-rootfs-part.img"

# Populated by build_uboot; kept as mutable globals because the tree lives
# under WORK_DIR and is rebuilt every run.
UB_SPL=""
UB_ITB=""

readonly DOWNLOAD_CONNECT_TIMEOUT_SECS="${MINIMAL_LINUX_DOWNLOAD_CONNECT_TIMEOUT_SECS:-30}"
readonly DOWNLOAD_TIMEOUT_SECS="${MINIMAL_LINUX_DOWNLOAD_TIMEOUT_SECS:-900}"

QEMU_PID=""

# Fetch a pinned archive, verifying its digest on every run so a corrupted or
# substituted download fails the job instead of silently changing the test.
download_asset() {
  local url=$1
  local destination=$2
  local digest=$3
  local temp

  if [[ -f "$destination" ]] && printf '%s  %s\n' "$digest" "$destination" | sha256sum --check --status; then
    echo "Using cached $(basename "$destination")" >&2
    return
  fi

  echo "Downloading $url" >&2
  mkdir -p "$(dirname "$destination")"
  temp=$(mktemp "${destination}.part.XXXXXX")

  if ! curl --fail --location \
    --connect-timeout "$DOWNLOAD_CONNECT_TIMEOUT_SECS" \
    --max-time "$DOWNLOAD_TIMEOUT_SECS" \
    --retry 3 \
    --retry-all-errors \
    --retry-max-time "$DOWNLOAD_TIMEOUT_SECS" \
    --output "$temp" \
    "$url"; then
    rm -f "$temp"
    return 1
  fi

  if ! printf '%s  %s\n' "$digest" "$temp" | sha256sum --check --status; then
    echo "Checksum mismatch for $url" >&2
    rm -f "$temp"
    return 1
  fi

  mv "$temp" "$destination"
}

# Unpack the pinned Bootlin riscv32 toolchain unless an extracted one is
# already present. `cross_gcc` prints the cross prefix so callers fail early
# when the toolchain is broken instead of deep inside a kernel build.
prepare_toolchain() {
  if [[ ! -x "${TOOLCHAIN_DIR}/bin/${CROSS_PREFIX}gcc" ]]; then
    local tarball="${WORK_DIR}/${TOOLCHAIN_RELEASE}.tar.xz"
    mkdir -p "$WORK_DIR" "$CACHE_DIR"
    download_asset "$TOOLCHAIN_URL" "$tarball" "$TOOLCHAIN_SHA256"
    rm -rf "$TOOLCHAIN_DIR"
    tar -xJf "$tarball" -C "$CACHE_DIR"
  fi
  export PATH="${TOOLCHAIN_DIR}/bin:${PATH}"
  echo "Using $("${CROSS_PREFIX}gcc" -dumpversion) from ${TOOLCHAIN_DIR}" >&2
}

# `make Image` stops short of modules and device trees, neither of which this
# boot test loads.
prepare_kernel() {
  local tarball="${WORK_DIR}/linux-${KERNEL_VERSION}.tar.xz"
  local tree="${WORK_DIR}/linux-${KERNEL_VERSION}"
  local option
  local -a required_options

  if [[ -s "$KERNEL_IMAGE" ]]; then
    echo "Using cached Linux ${KERNEL_VERSION} kernel image" >&2
    return
  fi

  mkdir -p "$WORK_DIR" "$CACHE_DIR"
  download_asset "$KERNEL_URL" "$tarball" "$KERNEL_SHA256"

  rm -rf "$tree"
  tar -xJf "$tarball" -C "$WORK_DIR"
  make -C "$tree" ARCH=riscv CROSS_COMPILE="$CROSS_PREFIX" rv32_defconfig

  # The rv32 defconfig is expected to carry everything this boot needs, so
  # check it rather than trust it: if a defconfig revision drops one of these,
  # the job should say which option went missing instead of booting into an
  # unexplained silence.
  # The sbi path needs an initramfs; the u-boot path boots off an ext4
  # partition on a virtio-mmio block device (the `-device virtio-blk-device`
  # QEMU attaches below) and a GPT table (EFI_PARTITION).
  if [[ "$BOOT_MODE" = u-boot ]]; then
    required_options=(EXT4_FS VIRTIO_BLK VIRTIO_MMIO EFI_PARTITION SERIAL_8250_CONSOLE DEVTMPFS)
  else
    required_options=(BLK_DEV_INITRD SERIAL_8250_CONSOLE DEVTMPFS)
  fi
  for option in "${required_options[@]}"; do
    if [[ $("${tree}/scripts/config" --file "${tree}/.config" --state "$option") != y ]]; then
      echo "rv32 defconfig no longer enables ${option}" >&2
      return 1
    fi
  done

  make -C "$tree" ARCH=riscv CROSS_COMPILE="$CROSS_PREFIX" -j"$(nproc)" Image

  cp "${tree}/arch/riscv/boot/Image" "$KERNEL_IMAGE"
}

# BusyBox is linked statically because the initramfs carries no shared
# libraries. `make install` lays out the applet symlinks under _install, which
# is what gets cached.
prepare_busybox() {
  local tarball="${WORK_DIR}/busybox-${BUSYBOX_VERSION}.tar.bz2"
  local tree="${WORK_DIR}/busybox-${BUSYBOX_VERSION}"

  if [[ -x "${BUSYBOX_INSTALL}/bin/busybox" ]]; then
    echo "Using cached BusyBox ${BUSYBOX_VERSION} installation" >&2
    return
  fi

  mkdir -p "$WORK_DIR" "$CACHE_DIR"
  download_asset "$BUSYBOX_URL" "$tarball" "$BUSYBOX_SHA256"

  rm -rf "$tree"
  tar -xjf "$tarball" -C "$WORK_DIR"
  make -C "$tree" ARCH=riscv CROSS_COMPILE="$CROSS_PREFIX" defconfig

  # Settings -> Build Options -> Build static binary (no shared libs). defconfig
  # leaves CONFIG_STATIC unset; the build reads .config directly, so flipping
  # the line is enough and no oldconfig pass is needed.
  sed -i 's/^# CONFIG_STATIC is not set$/CONFIG_STATIC=y/' "${tree}/.config"
  grep -q '^CONFIG_STATIC=y' "${tree}/.config" || echo 'CONFIG_STATIC=y' >>"${tree}/.config"

  # `tc` still uses the CBQ scheduler constants, which Linux 6.8 dropped from
  # its UAPI headers, so the applet no longer compiles against a current
  # toolchain (busybox 1.37.0 is affected too). An initramfs smoke test has no
  # use for traffic control, so drop the applet rather than pin old headers.
  sed -i 's/^CONFIG_TC=y$/# CONFIG_TC is not set/' "${tree}/.config"
  grep -q '^# CONFIG_TC is not set$' "${tree}/.config" || echo '# CONFIG_TC is not set' >>"${tree}/.config"

  make -C "$tree" ARCH=riscv CROSS_COMPILE="$CROSS_PREFIX" -j"$(nproc)"
  make -C "$tree" ARCH=riscv CROSS_COMPILE="$CROSS_PREFIX" install

  rm -rf "$BUSYBOX_INSTALL"
  cp -a "${tree}/_install" "$BUSYBOX_INSTALL"
}

# An initramfs is unpacked into the initial root filesystem, so the kernel runs
# /init directly. The marker proves a real userspace started; powering off
# afterwards lets the job finish without waiting out the boot timeout. The
# initramfs is rebuilt every run so edits to /init cannot go stale in a cache.
build_initramfs() {
  local rootfs="${WORK_DIR}/rootfs"

  rm -rf "$rootfs"
  mkdir -p "$rootfs" "$WORK_DIR"
  cp -a "${BUSYBOX_INSTALL}/." "$rootfs/"
  mkdir -p "$rootfs/proc" "$rootfs/sys" "$rootfs/dev"

  # The assertions make the marker mean something specific: an RV32 kernel
  # reached userspace and mounted a working /proc, not merely that some shell
  # ran. `set -eu` turns a failed assertion into a non-zero exit, which the
  # kernel reports as "Attempted to kill init" and the wait loop recognises as
  # a boot failure.
  cat >"$rootfs/init" <<EOF
#!/bin/sh
set -eu

/bin/mount -t proc none /proc
/bin/mount -t sysfs none /sys
/bin/mount -t devtmpfs none /dev

/usr/bin/test "\$(/bin/uname -m)" = riscv32
/usr/bin/test -r /proc/version

echo "${SMOKE_MARKER} \$(/bin/uname -r)"
/sbin/poweroff -f
EOF
  chmod +x "$rootfs/init"

  (cd "$rootfs" && find . -print0 | cpio --null --create --format=newc --quiet | gzip -9) >"$INITRAMFS"
}

# Build a GPT-partitioned disk image whose first (ext4) partition holds the
# kernel image and the BusyBox root filesystem. The image is rebuilt every run
# so edits to rcS cannot go stale in a cache.
#
# The filesystem is populated without a loop device: mkfs.ext4 -d fills a
# partition-sized file directly, which is then copied into the image at the
# partition offset. The partition stops 1 MiB short of the image end so the
# copy cannot overwrite the backup GPT header.
make_rootfs_image() {
  local rootfs="${WORK_DIR}/rootfs"

  rm -rf "$rootfs" "$DISK_IMAGE" "$DISK_PARTITION"
  mkdir -p "$WORK_DIR" "$rootfs"
  cp "$KERNEL_IMAGE" "$rootfs/Image"
  cp -a "${BUSYBOX_INSTALL}/." "$rootfs/"
  mkdir -p "$rootfs/proc" "$rootfs/sys" "$rootfs/dev" "$rootfs/etc/init.d"

  # busybox init runs /etc/init.d/rcS. The assertions make the marker mean
  # something specific: an RV32 kernel reached userspace and mounted a working
  # /proc, not merely that some shell ran.
  cat >"$rootfs/etc/init.d/rcS" <<'EOF'
#!/bin/sh
mount -t proc none /proc
mount -t sysfs none /sys
/sbin/mdev -s
set -e
[ "$(uname -m)" = riscv32 ]
[ -r /proc/version ]
echo "RUSTSBI-SMOKE-OK"
EOF
  chmod +x "$rootfs/etc/init.d/rcS"

  qemu-img create -q "$DISK_IMAGE" 100M
  parted -s "$DISK_IMAGE" mklabel gpt
  parted -s "$DISK_IMAGE" mkpart primary ext4 1MiB 99MiB
  parted -s "$DISK_IMAGE" set 1 boot on

  truncate -s 98M "$DISK_PARTITION"
  mkfs.ext4 -q -F -d "$rootfs" "$DISK_PARTITION"
  dd if="$DISK_PARTITION" of="$DISK_IMAGE" bs=1M seek=1 conv=notrunc status=none
}

# Build U-Boot with the RustSBI firmware embedded as the OpenSBI payload. The
# build products are deliberately not cached: the SPL embeds the firmware, so
# they must be rebuilt from the commit under test every run.
build_uboot() {
  local tarball="${WORK_DIR}/u-boot-${UB_VERSION}.tar.gz"
  local tree="${WORK_DIR}/u-boot-${UB_VERSION}"
  local rustsbi_abs

  # `make -C` runs inside the U-Boot tree, so OPENSBI must be an absolute
  # path; a relative path would be resolved against the tree, not the repo.
  rustsbi_abs="$(readlink -f "$RUSTSBI")"

  mkdir -p "$WORK_DIR"
  download_asset "$UB_URL" "$tarball" "$UB_SHA256"

  rm -rf "$tree"
  tar -xzf "$tarball" -C "$WORK_DIR"

  make -C "$tree" ARCH=riscv CROSS_COMPILE="$CROSS_PREFIX" \
    OPENSBI="$rustsbi_abs" qemu-riscv32_spl_defconfig

  # Set the default boot command non-interactively, equivalent to the
  # menuconfig step in the repository guide. The single quotes keep
  # ${fdtcontroladdr} literal so U-Boot expands it at runtime. 0x80400000 is
  # where QEMU's RV32 virt machine starts raw kernel images, and it sits far
  # enough above the ITB load address not to collide during ext4load.
  "${tree}/scripts/config" --file "${tree}/.config" --enable USE_BOOTCOMMAND
  # shellcheck disable=SC2016
  "${tree}/scripts/config" --file "${tree}/.config" --set-str BOOTCOMMAND \
    'ext4load virtio 0:1 80400000 Image; setenv bootargs root=/dev/vda1 rw console=ttyS0; booti 0x80400000 - ${fdtcontroladdr}'

  make -C "$tree" ARCH=riscv CROSS_COMPILE="$CROSS_PREFIX" \
    OPENSBI="$rustsbi_abs" -j"$(nproc)"

  UB_SPL="${tree}/spl/u-boot-spl"
  UB_ITB="${tree}/u-boot.itb"
}

check_prerequisites() {
  test -s "$RUSTSBI" || {
    echo "Missing $RUSTSBI; run 'cargo prototyper build --target riscv32imac-unknown-none-elf' first" >&2
    return 1
  }
  qemu-system-riscv32 --version
}

stop_qemu() {
  if [[ -n "$QEMU_PID" ]] && kill -0 "$QEMU_PID" 2>/dev/null; then
    kill "$QEMU_PID" 2>/dev/null || true
    wait "$QEMU_PID" 2>/dev/null || true
  fi
}

cleanup() {
  stop_qemu
}

start_qemu_sbi() {
  mkdir -p "$LOG_DIR"
  # Truncate any stale log from a previous run before QEMU starts, so the
  # wait loop cannot mistake an old success marker for this boot's.
  : >"$LOG_FILE"
  qemu-system-riscv32 \
    -machine virt \
    -smp 1 \
    -m 512M \
    -nographic \
    -no-reboot \
    -bios "$RUSTSBI" \
    -kernel "$KERNEL_IMAGE" \
    -initrd "$INITRAMFS" \
    >"$LOG_FILE" 2>&1 &
  QEMU_PID=$!
}

start_qemu_uboot() {
  mkdir -p "$LOG_DIR"
  : >"$LOG_FILE"
  qemu-system-riscv32 \
    -machine virt \
    -smp 1 \
    -m 256M \
    -nographic \
    -no-reboot \
    -bios "$UB_SPL" \
    -device loader,file="$UB_ITB",addr=0x80200000 \
    -blockdev driver=file,filename="$DISK_IMAGE",node-name=hd0 \
    -device virtio-blk-device,drive=hd0 \
    >"$LOG_FILE" 2>&1 &
  QEMU_PID=$!
}

userspace_is_ready() {
  grep -Fq "$SMOKE_MARKER" "$LOG_FILE"
}

boot_has_failed() {
  grep -Eq "$BOOT_FAILURE_PATTERN" "$LOG_FILE"
}

report_boot_failure() {
  echo "Linux failed to boot:" >&2
  grep -E --max-count=5 "$BOOT_FAILURE_PATTERN" "$LOG_FILE" >&2 || true
  tail -n 120 "$LOG_FILE" || true
}

report_early_exit() {
  local qemu_exit
  set +e
  wait "$QEMU_PID"
  qemu_exit=$?
  set -e

  echo "QEMU exited before Linux reached userspace (exit=${qemu_exit})" >&2
  tail -n 120 "$LOG_FILE" || true
}

wait_for_userspace() {
  local elapsed
  for ((elapsed = 0; elapsed < BOOT_TIMEOUT_SECS; elapsed++)); do
    # The marker is checked first so a boot that succeeds just before a late
    # panic is still reported as the success it was.
    if userspace_is_ready; then
      return 0
    fi
    if boot_has_failed; then
      report_boot_failure
      return 1
    fi
    if ! kill -0 "$QEMU_PID" 2>/dev/null; then
      # QEMU may have printed the marker and powered off since our last read.
      if userspace_is_ready; then
        return 0
      fi
      report_early_exit
      return 1
    fi
    sleep 1
  done

  # Check once more after the final sleep, including the timeout boundary.
  if userspace_is_ready; then
    return 0
  fi

  echo "Linux did not reach userspace within ${BOOT_TIMEOUT_SECS}s" >&2
  tail -n 120 "$LOG_FILE" || true
  return 1
}

main() {
  trap cleanup EXIT

  check_prerequisites
  prepare_toolchain
  prepare_kernel
  prepare_busybox
  case "$BOOT_MODE" in
    u-boot)
      make_rootfs_image
      build_uboot
      start_qemu_uboot
      ;;
    sbi)
      build_initramfs
      start_qemu_sbi
      ;;
  esac
  wait_for_userspace

  echo "RustSBI booted Linux ${KERNEL_VERSION} to RV32 userspace successfully (${BOOT_MODE})"
  echo "QEMU log: ${LOG_FILE}"
}

main "$@"
