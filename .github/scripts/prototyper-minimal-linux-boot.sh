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
# Linux and BusyBox are built from checksum-pinned release archives. Only the
# build *products* are kept under CACHE_DIR, so the workflow restores a small
# cache and skips compilation entirely on a hit; the archives themselves are
# verified on every download. RustSBI is rebuilt from the commit under test and
# is never cached.
#
# Requires: `cargo prototyper build` to have produced the firmware, plus
# qemu-system-riscv64, riscv64-linux-gnu-gcc, cpio, xz and a host toolchain.

set -euo pipefail

readonly KERNEL_VERSION="6.12.110"
readonly KERNEL_URL="https://cdn.kernel.org/pub/linux/kernel/v6.x/linux-${KERNEL_VERSION}.tar.xz"
readonly KERNEL_SHA256="8cee19e1839bb6ff4d5254d761933ae6ab670492d5ed030e09a80538320d5c4c"

readonly BUSYBOX_VERSION="1.36.1"
readonly BUSYBOX_URL="https://busybox.net/downloads/busybox-${BUSYBOX_VERSION}.tar.bz2"
readonly BUSYBOX_SHA256="b8cc24c9574d809e7279c3be349795c5d5ceb6fdf19ca709f80cde50e47de314"

readonly CROSS_COMPILE="riscv64-linux-gnu-"
readonly SMOKE_MARKER="RUSTSBI-SMOKE-OK"

# Text that means the boot already went wrong. A panic halts the machine while
# QEMU stays alive, so without this the job would sit out the whole timeout and
# then report only that the marker never appeared.
readonly BOOT_FAILURE_PATTERN="Kernel panic|not syncing|Attempted to kill init"

readonly CACHE_DIR="${MINIMAL_LINUX_CACHE_DIR:-.cache/minimal-linux}"
readonly WORK_DIR="${MINIMAL_LINUX_WORK_DIR:-.minimal-linux/work}"
readonly RUSTSBI="${MINIMAL_LINUX_RUSTSBI:-target/riscv64gc-unknown-none-elf/release/rustsbi-prototyper-dynamic.elf}"
readonly LOG_DIR="${QEMU_LOG_DIR:-qemu-logs}"
readonly LOG_FILE="${LOG_DIR}/prototyper-minimal-linux.log"

readonly KERNEL_IMAGE="${CACHE_DIR}/linux-${KERNEL_VERSION}-Image"
readonly BUSYBOX_INSTALL="${CACHE_DIR}/busybox-${BUSYBOX_VERSION}-install"
readonly INITRAMFS="${WORK_DIR}/initramfs.cpio.gz"

readonly BOOT_TIMEOUT_SECS="${MINIMAL_LINUX_BOOT_TIMEOUT_SECS:-180}"
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

# `make Image` stops short of modules and device trees, neither of which this
# boot test loads.
prepare_kernel() {
  local tarball="${WORK_DIR}/linux-${KERNEL_VERSION}.tar.xz"
  local tree="${WORK_DIR}/linux-${KERNEL_VERSION}"
  local option

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
  for option in BLK_DEV_INITRD SERIAL_8250_CONSOLE DEVTMPFS; do
    if [[ $("${tree}/scripts/config" --file "${tree}/.config" --state "$option") != y ]]; then
      echo "riscv defconfig no longer enables ${option}" >&2
      return 1
    fi
  done

  make -C "$tree" ARCH=riscv CROSS_COMPILE="$CROSS_COMPILE" -j"$(nproc)" Image

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

echo "${SMOKE_MARKER} \$(/bin/uname -r)"
/sbin/poweroff -f
EOF
  chmod +x "$rootfs/init"

  (cd "$rootfs" && find . -print0 | cpio --null --create --format=newc --quiet | gzip -9) >"$INITRAMFS"
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
}

start_qemu() {
  mkdir -p "$LOG_DIR"
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
  build_initramfs
  start_qemu
  wait_for_userspace

  echo "RustSBI booted Linux ${KERNEL_VERSION} to userspace successfully"
  echo "QEMU log: ${LOG_FILE}"
}

main "$@"
