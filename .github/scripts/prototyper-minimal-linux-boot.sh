#!/usr/bin/env bash
#
# Boot a minimal Linux userspace through RustSBI Prototyper in QEMU.
#
# RustSBI's dynamic firmware follows the fw_dynamic handoff convention, so QEMU
# starts the kernel directly: `-bios` points at the firmware and `-kernel` at
# the flattened kernel image, with QEMU handing the kernel entry point to the
# firmware through the dynamic info structure. No intermediate bootloader takes
# part, which keeps the boot path short and the CI job fast.
#
# A U-Boot-based boot path is selected with `$0 u-boot`: the SPL embeds
# RustSBI Prototyper as its OpenSBI payload, so the chain is QEMU -> u-boot-spl
# -> RustSBI -> u-boot.itb -> Linux. U-Boot loads the kernel off an ext4
# partition with `ext4load` and starts it with `booti`, matching the repository
# guide firmware/docs/booting-linux-kernel-in-qemu-using-uboot-and-rustsbi.md.
#
# An EDK II-based boot path is selected with `$0 edk2`: QEMU starts RustSBI's
# dynamic firmware, which launches the pinned EDK II RISC-V QEMU payload. EDK
# II then loads Linux through the EFI stub. The initramfs verifies that
# /sys/firmware/efi exists before it prints the success marker.
#
# Linux and BusyBox are built from checksum-pinned release archives. Only the
# build *products* are kept under CACHE_DIR, so the workflow restores a small
# cache and skips compilation entirely on a hit; the archives themselves are
# verified on every download. RustSBI is rebuilt from the commit under test and
# is never cached.
#
# The U-Boot build is never cached: the SPL embeds the RustSBI firmware, so it
# must be rebuilt from the commit under test every run.
#
# Requires: `cargo prototyper build` to have produced the firmware, plus
# qemu-system-riscv64, riscv64-linux-gnu-gcc, cpio, xz and a host toolchain.
# The u-boot path additionally needs swig, python3-dev, parted, e2fsprogs and
# qemu-utils. The edk2 path additionally needs acpica-tools, nasm and uuid-dev.

set -euo pipefail

if (( $# > 1 )); then
  echo "Usage: $0 [sbi|u-boot|edk2]" >&2
  exit 2
fi

readonly BOOT_MODE="${1:-sbi}"
case "$BOOT_MODE" in
  sbi | u-boot | edk2) ;;
  *)
    echo "Unknown boot mode: ${BOOT_MODE}" >&2
    echo "Usage: $0 [sbi|u-boot|edk2]" >&2
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

readonly EDK2_VERSION="edk2-stable202505"
readonly EDK2_COMMIT="6951dfe7d59d144a3a980bd7eda699db2d8554ac"
readonly EDK2_URL="https://github.com/tianocore/edk2.git"

readonly CROSS_COMPILE="riscv64-linux-gnu-"
readonly SMOKE_MARKER="RUSTSBI-SMOKE-OK"

# Text that means the boot already went wrong. A panic halts the machine while
# QEMU stays alive, so without this the job would sit out the whole timeout and
# then report only that the marker never appeared.
readonly BOOT_FAILURE_PATTERN="Kernel panic|not syncing|Attempted to kill init"

readonly CACHE_DIR="${MINIMAL_LINUX_CACHE_DIR:-.cache/minimal-linux}"
readonly WORK_DIR="${MINIMAL_LINUX_WORK_DIR:-.minimal-linux/work}"
readonly LOG_DIR="${QEMU_LOG_DIR:-qemu-logs}"
readonly LOG_FILE="${LOG_DIR}/prototyper-minimal-linux-${BOOT_MODE}.log"

# The sbi path boots the dynamic firmware directly; the u-boot path embeds the
# flattened binary in the SPL; and the edk2 path boots the dynamic binary so
# EDK II can receive the next-stage handoff. Bootloader paths get more time.
if [[ "$BOOT_MODE" = u-boot ]]; then
  readonly RUSTSBI="${MINIMAL_LINUX_RUSTSBI_BIN:-target/riscv64gc-unknown-none-elf/release/rustsbi-prototyper.bin}"
  readonly BOOT_TIMEOUT_SECS="${MINIMAL_LINUX_BOOT_TIMEOUT_SECS:-240}"
elif [[ "$BOOT_MODE" = edk2 ]]; then
  readonly RUSTSBI="${MINIMAL_LINUX_RUSTSBI_DYNAMIC_BIN:-target/riscv64gc-unknown-none-elf/release/rustsbi-prototyper-dynamic.bin}"
  readonly BOOT_TIMEOUT_SECS="${MINIMAL_LINUX_BOOT_TIMEOUT_SECS:-240}"
else
  readonly RUSTSBI="${MINIMAL_LINUX_RUSTSBI:-target/riscv64gc-unknown-none-elf/release/rustsbi-prototyper-dynamic.elf}"
  readonly BOOT_TIMEOUT_SECS="${MINIMAL_LINUX_BOOT_TIMEOUT_SECS:-180}"
fi

readonly KERNEL_IMAGE="${CACHE_DIR}/linux-${KERNEL_VERSION}-Image"
readonly BUSYBOX_INSTALL="${CACHE_DIR}/busybox-${BUSYBOX_VERSION}-install"
readonly INITRAMFS="${WORK_DIR}/initramfs.cpio.gz"
readonly DISK_IMAGE="${WORK_DIR}/linux-rootfs.img"
readonly EDK2_CACHE="${CACHE_DIR}/edk2-${EDK2_COMMIT}"
readonly EDK2_CODE="${EDK2_CACHE}/RISCV_VIRT_CODE.fd"
readonly EDK2_VARS_TEMPLATE="${EDK2_CACHE}/RISCV_VIRT_VARS.fd"
readonly EDK2_VARS="${WORK_DIR}/RISCV_VIRT_VARS.fd"

# Populated by build_uboot; kept as mutable globals because the tree lives
# under WORK_DIR and is rebuilt every run.
UB_SPL=""
UB_ITB=""

readonly DOWNLOAD_CONNECT_TIMEOUT_SECS="${MINIMAL_LINUX_DOWNLOAD_CONNECT_TIMEOUT_SECS:-30}"
readonly DOWNLOAD_TIMEOUT_SECS="${MINIMAL_LINUX_DOWNLOAD_TIMEOUT_SECS:-900}"

QEMU_PID=""
# The loop device and mount point that make_rootfs_image opens, tracked so the
# EXIT cleanup can release them if a setup step fails under `set -e`.
LOOP_DEVICE=""
ROOTFS_MOUNT=""

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
  make -C "$tree" ARCH=riscv CROSS_COMPILE="$CROSS_COMPILE" defconfig

  # The riscv defconfig is expected to carry everything this boot needs, so
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
    if [[ "$BOOT_MODE" = edk2 ]]; then
      required_options+=(EFI EFI_STUB)
    fi
  fi
  for option in "${required_options[@]}"; do
    if [[ $("${tree}/scripts/config" --file "${tree}/.config" --state "$option") != y ]]; then
      echo "riscv defconfig no longer enables ${option}" >&2
      return 1
    fi
  done

  make -C "$tree" ARCH=riscv CROSS_COMPILE="$CROSS_COMPILE" -j"$(nproc)" Image

  cp "${tree}/arch/riscv/boot/Image" "$KERNEL_IMAGE"
}

# Build EDK II's RISC-V QEMU payload from an exact commit. The firmware volumes
# are the only EDK II products cached; the writable variables volume is copied
# for every boot so one run cannot contaminate the next cache restore.
prepare_edk2() {
  local workspace
  workspace="$(realpath -m "${WORK_DIR}/edk2-workspace")"
  local source="${workspace}/edk2"
  local output="${workspace}/Build/RiscVVirtQemu/RELEASE_GCC5/FV"
  local head

  if [[ $(stat -c %s "$EDK2_CODE" 2>/dev/null || true) == 33554432 &&
        $(stat -c %s "$EDK2_VARS_TEMPLATE" 2>/dev/null || true) == 33554432 ]]; then
    echo "Using cached EDK II ${EDK2_VERSION} firmware" >&2
    return
  fi

  rm -rf "$workspace" "$EDK2_CACHE"
  mkdir -p "$workspace" "$EDK2_CACHE"

  git init --quiet "$source"
  git -C "$source" remote add origin "$EDK2_URL"
  git -C "$source" fetch --quiet --depth=1 origin "$EDK2_COMMIT"
  git -C "$source" checkout --quiet --detach FETCH_HEAD
  head=$(git -C "$source" rev-parse HEAD)
  if [[ "$head" != "$EDK2_COMMIT" ]]; then
    echo "EDK II checkout mismatch: expected ${EDK2_COMMIT}, got ${head}" >&2
    return 1
  fi
  # EDK II's top-level submodules contain every source used by this platform.
  # Their own nested development/test submodules are not build inputs, so do
  # not recursively clone those unrelated repositories into a CI smoke test.
  git -C "$source" submodule update --init --depth=1

  (
    cd "$workspace"
    export WORKSPACE="$PWD"
    export PACKAGES_PATH="$source"
    export EDK_TOOLS_PATH="${source}/BaseTools"
    export GCC5_RISCV64_PREFIX="$CROSS_COMPILE"

    # edksetup and BaseTools deliberately communicate through environment
    # variables, so keep these commands in one subshell. edksetup also probes
    # optional unset variables, which is incompatible with this script's
    # `set -u`; disable nounset only while sourcing that upstream script.
    set +u
    # shellcheck disable=SC1091
    source "${source}/edksetup.sh" --reconfig
    set -u
    make -C "${source}/BaseTools" -j"$(nproc)"
    set +u
    # shellcheck disable=SC1091
    source "${source}/edksetup.sh" BaseTools
    set -u
    build -a RISCV64 -b RELEASE \
      -p OvmfPkg/RiscVVirt/RiscVVirtQemu.dsc \
      -t GCC5
  )

  cp "${output}/RISCV_VIRT_CODE.fd" "$EDK2_CODE"
  cp "${output}/RISCV_VIRT_VARS.fd" "$EDK2_VARS_TEMPLATE"
  truncate -s 32M "$EDK2_CODE" "$EDK2_VARS_TEMPLATE"
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
  make -C "$tree" ARCH=riscv CROSS_COMPILE="$CROSS_COMPILE" defconfig

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

  make -C "$tree" ARCH=riscv CROSS_COMPILE="$CROSS_COMPILE" -j"$(nproc)"
  make -C "$tree" ARCH=riscv CROSS_COMPILE="$CROSS_COMPILE" install

  rm -rf "$BUSYBOX_INSTALL"
  cp -a "${tree}/_install" "$BUSYBOX_INSTALL"
}

# An initramfs is unpacked into the initial root filesystem, so the kernel runs
# /init directly. The marker proves a real userspace started; powering off
# afterwards lets the job finish without waiting out the boot timeout. The
# initramfs is rebuilt every run so edits to /init cannot go stale in a cache.
build_initramfs() {
  local rootfs="${WORK_DIR}/rootfs"
  local efi_assertion=""

  if [[ "$BOOT_MODE" = edk2 ]]; then
    # Prove that the kernel entered through the EFI stub instead of merely
    # reaching the same userspace by another boot path.
    efi_assertion="/usr/bin/test -d /sys/firmware/efi"
  fi

  rm -rf "$rootfs"
  mkdir -p "$rootfs" "$WORK_DIR"
  cp -a "${BUSYBOX_INSTALL}/." "$rootfs/"
  mkdir -p "$rootfs/proc" "$rootfs/sys" "$rootfs/dev"

  # The assertions make the marker mean something specific: a riscv64 kernel
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

/usr/bin/test "\$(/bin/uname -m)" = riscv64
/usr/bin/test -r /proc/version
${efi_assertion}

echo "${SMOKE_MARKER} \$(/bin/uname -r)"
/sbin/poweroff -f
EOF
  chmod +x "$rootfs/init"

  (cd "$rootfs" && find . -print0 | cpio --null --create --format=newc --quiet | gzip -9) >"$INITRAMFS"
}

# Build a 1GiB GPT-partitioned disk image whose first (ext4) partition holds
# the kernel image and the BusyBox root filesystem. The image is rebuilt every
# run so edits to rcS cannot go stale in a cache.
make_rootfs_image() {
  local rootfs="${WORK_DIR}/rootfs"
  local loop part start end size

  rm -rf "$rootfs" "$DISK_IMAGE"
  mkdir -p "$WORK_DIR"

  qemu-img create -q "$DISK_IMAGE" 1g
  parted -s "$DISK_IMAGE" mklabel gpt
  parted -s "$DISK_IMAGE" mkpart primary ext4 1MiB 100%
  parted -s "$DISK_IMAGE" set 1 boot on

  # Read the exact byte range of partition 1. The sizelimit must match the
  # partition exactly: if mkfs runs over a loop device that reaches the end of
  # the file, it overwrites the backup GPT header and the kernel later rejects
  # the filesystem as having bad geometry. Using offset+sizelimit also avoids
  # needing /dev/loopNpM partition nodes, which udev-less containers lack.
  part=$(parted -s "$DISK_IMAGE" unit B print | awk '/^ 1 /{print $2, $3}')
  start=$(echo "$part" | awk '{gsub(/B/,"",$1); print $1}')
  end=$(echo "$part" | awk '{gsub(/B/,"",$2); print $2}')
  size=$((end - start + 1))

  loop=$(priv losetup --find --show --offset "$start" --sizelimit "$size" "$DISK_IMAGE")
  LOOP_DEVICE="$loop"
  priv mkfs.ext4 -q "$loop"

  mkdir -p "$rootfs"
  priv mount "$loop" "$rootfs"
  ROOTFS_MOUNT="$rootfs"

  # After `sudo mount` the mount point is owned by root, so a non-root CI
  # runner cannot write to it. Hand it to the current user for the copies
  # below (a no-op when already running as root in a local container).
  priv chown "$(id -u):$(id -g)" "$rootfs"

  cp "$KERNEL_IMAGE" "$rootfs/Image"
  cp -a "${BUSYBOX_INSTALL}/." "$rootfs/"
  mkdir -p "$rootfs/proc" "$rootfs/sys" "$rootfs/dev" "$rootfs/etc/init.d"

  # busybox init runs /etc/init.d/rcS. The assertions make the marker mean
  # something specific: a riscv64 kernel reached userspace and mounted a
  # working /proc, not merely that some shell ran.
  cat >"$rootfs/etc/init.d/rcS" <<'EOF'
#!/bin/sh
mount -t proc none /proc
mount -t sysfs none /sys
/sbin/mdev -s
set -e
[ "$(uname -m)" = riscv64 ]
[ -r /proc/version ]
echo "RUSTSBI-SMOKE-OK"
EOF
  chmod +x "$rootfs/etc/init.d/rcS"

  priv umount "$rootfs"
  ROOTFS_MOUNT=""
  priv losetup -d "$loop"
  LOOP_DEVICE=""
  rmdir "$rootfs"
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

  make -C "$tree" ARCH=riscv CROSS_COMPILE="$CROSS_COMPILE" \
    OPENSBI="$rustsbi_abs" qemu-riscv64_spl_defconfig

  # Set the default boot command non-interactively, equivalent to the
  # menuconfig step in the repository guide. The single quotes keep
  # ${fdtcontroladdr} literal so U-Boot expands it at runtime.
  "${tree}/scripts/config" --file "${tree}/.config" --enable USE_BOOTCOMMAND
  # shellcheck disable=SC2016
  "${tree}/scripts/config" --file "${tree}/.config" --set-str BOOTCOMMAND \
    'ext4load virtio 0:1 84000000 Image; setenv bootargs root=/dev/vda1 rw console=ttyS0; booti 0x84000000 - ${fdtcontroladdr}'

  make -C "$tree" ARCH=riscv CROSS_COMPILE="$CROSS_COMPILE" \
    OPENSBI="$rustsbi_abs" -j"$(nproc)"

  UB_SPL="${tree}/spl/u-boot-spl"
  UB_ITB="${tree}/u-boot.itb"
}

# Run a command with elevated privileges when running as a non-root user (CI
# runners) while staying a plain call inside a root container (local Docker).
priv() {
  if [[ $EUID -eq 0 ]]; then
    "$@"
  else
    sudo "$@"
  fi
}

check_prerequisites() {
  test -s "$RUSTSBI" || {
    echo "Missing $RUSTSBI; run 'cargo prototyper build' first" >&2
    return 1
  }
  qemu-system-riscv64 --version
}

stop_qemu() {
  if [[ -n "$QEMU_PID" ]] && kill -0 "$QEMU_PID" 2>/dev/null; then
    kill "$QEMU_PID" 2>/dev/null || true
    wait "$QEMU_PID" 2>/dev/null || true
  fi
}

cleanup() {
  stop_qemu
  # Release any loop device or mount left behind by a failed setup step, so a
  # local rerun does not leak them and exhaust loop devices.
  if [[ -n "$ROOTFS_MOUNT" ]]; then
    priv umount "$ROOTFS_MOUNT" 2>/dev/null || true
  fi
  if [[ -n "$LOOP_DEVICE" ]]; then
    priv losetup -d "$LOOP_DEVICE" 2>/dev/null || true
  fi
}

start_qemu_sbi() {
  mkdir -p "$LOG_DIR"
  # Truncate any stale log from a previous run before QEMU starts, so the
  # wait loop cannot mistake an old success marker for this boot's.
  : >"$LOG_FILE"
  qemu-system-riscv64 \
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
  qemu-system-riscv64 \
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

start_qemu_edk2() {
  mkdir -p "$LOG_DIR"
  : >"$LOG_FILE"
  cp "$EDK2_VARS_TEMPLATE" "$EDK2_VARS"
  qemu-system-riscv64 \
    -machine virt,pflash0=pflash0,pflash1=pflash1,acpi=off \
    -smp 1 \
    -m 2G \
    -nographic \
    -no-reboot \
    -bios "$RUSTSBI" \
    -blockdev "node-name=pflash0,driver=file,read-only=on,filename=${EDK2_CODE}" \
    -blockdev "node-name=pflash1,driver=file,filename=${EDK2_VARS}" \
    -kernel "$KERNEL_IMAGE" \
    -initrd "$INITRAMFS" \
    -append "console=ttyS0 earlycon=uart8250,mmio,0x10000000 rdinit=/init" \
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
  prepare_kernel
  prepare_busybox
  case "$BOOT_MODE" in
    u-boot)
      make_rootfs_image
      build_uboot
      start_qemu_uboot
      ;;
    edk2)
      build_initramfs
      prepare_edk2
      start_qemu_edk2
      ;;
    sbi)
      build_initramfs
      start_qemu_sbi
      ;;
  esac
  wait_for_userspace

  echo "RustSBI booted Linux ${KERNEL_VERSION} to userspace successfully (${BOOT_MODE})"
  echo "QEMU log: ${LOG_FILE}"
}

main "$@"
