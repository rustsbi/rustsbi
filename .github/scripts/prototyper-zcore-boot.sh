#!/usr/bin/env bash
#
# Boot zCore through RustSBI Prototyper in QEMU and run a BusyBox shell
# command in its Linux-compatible userspace.
#
# Two boot paths are covered, selected by the first argument:
#
#   sbi     QEMU -> RustSBI (dynamic) -> zCore
#           QEMU loads the zCore binary and its rootfs image directly, the
#           same way zCore's own `cargo qemu` does with OpenSBI.
#
#   u-boot  QEMU -> u-boot-spl -> RustSBI -> U-Boot -> zCore
#           The SPL embeds RustSBI Prototyper as its OpenSBI payload. U-Boot
#           loads a legacy uImage of zCore and the rootfs from a FAT disk and
#           starts it with `bootm`.
#
# zCore is built from a pinned commit with the toolchain its repository pins.
# Its xtask normally downloads a musl cross toolchain and clones BusyBox at
# whatever revision is current; both are seeded here from checksum-pinned
# archives instead, so xtask finds them in place and never goes online for
# them. Only the build products (the kernel binary and rootfs image) are
# cached; RustSBI and U-Boot are rebuilt from the commit under test every run.
#
# zCore's init process comes from ROOTPROC on the command line, with `?`
# separating arguments. The command prints the kernel name and machine before
# the success marker, so the marker proves a riscv64 userspace process ran.
#
# Requires: `cargo prototyper build` to have produced the firmware, plus
# rustup, cargo-binutils, qemu-system-riscv64, qemu-img and a host C toolchain. The
# u-boot path additionally needs riscv64-linux-gnu-gcc, swig, python3-dev,
# libssl-dev, bison, flex, dosfstools and mtools.

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

readonly ZCORE_URL="https://github.com/rcore-os/zCore.git"
readonly ZCORE_COMMIT="8de51f4ee053bcaaf435897febf648bcb4d45479"

# The musl-cache release asset zCore's xtask downloads. It has not changed
# since 2022, but it is a mutable release asset, so pin its digest.
readonly MUSL_URL="https://github.com/YdrMaster/zCore/releases/download/musl-cache/riscv64-linux-musl-cross.tgz"
readonly MUSL_SHA256="db0bc413bd4a93f2012cc74b9ba0c4af29d8bc18b88e9c61998738ccb918604b"

readonly BUSYBOX_VERSION="1.36.1"
readonly BUSYBOX_URL="https://busybox.net/downloads/busybox-${BUSYBOX_VERSION}.tar.bz2"
readonly BUSYBOX_SHA256="b8cc24c9574d809e7279c3be349795c5d5ceb6fdf19ca709f80cde50e47de314"

readonly UB_VERSION="2024.04"
readonly UB_URL="https://github.com/u-boot/u-boot/archive/refs/tags/v${UB_VERSION}.tar.gz"
readonly UB_SHA256="d6b57ce574a0a0504a5b6596644ceacb7f77bde9353779bcf2fde07c4b9a2b92"

readonly CROSS_COMPILE="riscv64-linux-gnu-"
readonly SMOKE_MARKER="RUSTSBI-ZCORE-OK"
# `:` separates zCore boot options and `?` separates ROOTPROC arguments, so
# neither may appear inside the shell command.
readonly ZCORE_CMDLINE="LOG=warn:ROOTPROC=/bin/busybox?sh?-c?uname -sm; echo ${SMOKE_MARKER}"

# Text that means the boot already went wrong. zCore keeps running after a
# panic or a failed init process, so without this the job would sit out the
# whole timeout. `Unhandled exception` is U-Boot's trap handler, which is
# still installed if zCore faults before setting its own.
readonly BOOT_FAILURE_PATTERN='panicked at|exited with code|Unhandled exception|System shutdown scheduled due to RustSBI panic'

readonly CACHE_DIR="${ZCORE_CACHE_DIR:-.cache/zcore}"
readonly WORK_DIR="${ZCORE_WORK_DIR:-.zcore/work}"
readonly LOG_DIR="${QEMU_LOG_DIR:-qemu-logs}"
readonly LOG_FILE="${LOG_DIR}/prototyper-zcore-${BOOT_MODE}.log"

# The sbi path boots the dynamic firmware directly; the u-boot path embeds the
# flattened binary in the SPL.
if [[ "$BOOT_MODE" = u-boot ]]; then
  readonly RUSTSBI="${ZCORE_RUSTSBI_BIN:-target/riscv64gc-unknown-none-elf/release/rustsbi-prototyper.bin}"
else
  readonly RUSTSBI="${ZCORE_RUSTSBI:-target/riscv64gc-unknown-none-elf/release/rustsbi-prototyper-dynamic.elf}"
fi
readonly BOOT_TIMEOUT_SECS="${ZCORE_BOOT_TIMEOUT_SECS:-120}"

readonly ZCORE_TREE="${WORK_DIR}/zCore"
readonly ZCORE_BIN="${CACHE_DIR}/zcore-${ZCORE_COMMIT}.bin"
readonly ZCORE_ROOTFS="${CACHE_DIR}/zcore-${ZCORE_COMMIT}-rootfs.img"
readonly ZCORE_UIMAGE="${WORK_DIR}/zcore.uimg"
readonly DISK_IMAGE="${WORK_DIR}/zcore-boot.img"

# Populated by build_uboot; kept as mutable globals because the tree lives
# under WORK_DIR and is rebuilt every run.
UB_TREE=""
UB_SPL=""
UB_ITB=""

readonly DOWNLOAD_CONNECT_TIMEOUT_SECS="${ZCORE_DOWNLOAD_CONNECT_TIMEOUT_SECS:-30}"
readonly DOWNLOAD_TIMEOUT_SECS="${ZCORE_DOWNLOAD_TIMEOUT_SECS:-900}"

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

# Check out zCore at the pinned commit and build the kernel binary and the
# BusyBox rootfs image with zCore's own xtask.
prepare_zcore() {
  local head
  local origin

  if [[ -s "$ZCORE_BIN" && -s "$ZCORE_ROOTFS" ]]; then
    echo "Using cached zCore ${ZCORE_COMMIT} build" >&2
    return
  fi

  rm -rf "$ZCORE_TREE"
  mkdir -p "$WORK_DIR" "$CACHE_DIR"

  git init --quiet "$ZCORE_TREE"
  git -C "$ZCORE_TREE" remote add origin "$ZCORE_URL"
  git -C "$ZCORE_TREE" fetch --quiet --depth=1 origin "$ZCORE_COMMIT"
  git -C "$ZCORE_TREE" checkout --quiet --detach FETCH_HEAD
  head=$(git -C "$ZCORE_TREE" rev-parse HEAD)
  if [[ "$head" != "$ZCORE_COMMIT" ]]; then
    echo "zCore checkout mismatch: expected ${ZCORE_COMMIT}, got ${head}" >&2
    return 1
  fi

  # xtask skips a download whose destination already exists and a clone
  # whose directory already exists, so seed both with verified copies.
  origin="${ZCORE_TREE}/ignored/origin"
  download_asset "$MUSL_URL" "${origin}/archs/riscv64/riscv64-linux-musl-cross.tgz" "$MUSL_SHA256"
  download_asset "$BUSYBOX_URL" "${WORK_DIR}/busybox-${BUSYBOX_VERSION}.tar.bz2" "$BUSYBOX_SHA256"
  mkdir -p "${origin}/repos/busybox"
  tar -xjf "${WORK_DIR}/busybox-${BUSYBOX_VERSION}.tar.bz2" \
    -C "${origin}/repos/busybox" --strip-components=1

  (
    cd "$ZCORE_TREE"
    # zCore pins a different nightly from RustSBI; install the one named by
    # its rust-toolchain.toml so the cargo calls below pick it up.
    rustup toolchain install
    cargo image --arch riscv64
    cargo bin -m virt-riscv64 -o zcore.bin
  )

  cp "${ZCORE_TREE}/zcore.bin" "$ZCORE_BIN"
  cp "${ZCORE_TREE}/zCore/riscv64.img" "$ZCORE_ROOTFS"
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

  # zCore faults on a device tree at the top of RAM, where U-Boot keeps its
  # control tree, so move the tree to the standard fdt_addr_r first. The
  # single quotes keep the command line one hush word; `scripts/config`
  # rewrites the value with sed, so it must not contain `&`.
  "${tree}/scripts/config" --file "${tree}/.config" --enable USE_BOOTCOMMAND
  # shellcheck disable=SC2016
  "${tree}/scripts/config" --file "${tree}/.config" --set-str BOOTCOMMAND \
    "load virtio 0 \${kernel_addr_r} zcore.uimg; load virtio 0 \${ramdisk_addr_r} rootfs.img; setenv bootargs '${ZCORE_CMDLINE}'; fdt addr \${fdtcontroladdr}; fdt move \${fdtcontroladdr} \${fdt_addr_r} 0x10000; bootm \${kernel_addr_r} \${ramdisk_addr_r}:\${filesize} \${fdt_addr_r}"

  make -C "$tree" ARCH=riscv CROSS_COMPILE="$CROSS_COMPILE" \
    OPENSBI="$rustsbi_abs" -j"$(nproc)"

  UB_TREE="$tree"
  UB_SPL="${tree}/spl/u-boot-spl"
  UB_ITB="${tree}/u-boot.itb"
}

# zCore is a raw binary linked to run at 0x80200000, so wrap it in a legacy
# uImage that tells `bootm` where to copy it and that it takes the Linux boot
# protocol (hart ID in a0, device tree in a1). A FAT filesystem is written
# with mtools, so building the disk needs neither root nor loop devices.
make_boot_disk() {
  rm -f "$ZCORE_UIMAGE" "$DISK_IMAGE"
  "${UB_TREE}/tools/mkimage" -A riscv -O linux -T kernel -C none \
    -a 0x80200000 -e 0x80200000 -n zCore -d "$ZCORE_BIN" "$ZCORE_UIMAGE"

  truncate -s 64M "$DISK_IMAGE"
  mkfs.vfat -F 32 "$DISK_IMAGE" >/dev/null
  mcopy -i "$DISK_IMAGE" "$ZCORE_UIMAGE" ::zcore.uimg
  mcopy -i "$DISK_IMAGE" "$ZCORE_ROOTFS" ::rootfs.img
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

start_qemu_sbi() {
  mkdir -p "$LOG_DIR"
  # Truncate any stale log from a previous run before QEMU starts, so the
  # wait loop cannot mistake an old success marker for this boot's.
  : >"$LOG_FILE"
  qemu-system-riscv64 \
    -machine virt \
    -smp 2 \
    -m 2G \
    -nographic \
    -no-reboot \
    -bios "$RUSTSBI" \
    -kernel "$ZCORE_BIN" \
    -initrd "$ZCORE_ROOTFS" \
    -append "$ZCORE_CMDLINE" \
    >"$LOG_FILE" 2>&1 &
  QEMU_PID=$!
}

start_qemu_uboot() {
  mkdir -p "$LOG_DIR"
  : >"$LOG_FILE"
  qemu-system-riscv64 \
    -machine virt \
    -smp 2 \
    -m 2G \
    -nographic \
    -no-reboot \
    -bios "$UB_SPL" \
    -device loader,file="$UB_ITB",addr=0x80200000 \
    -blockdev driver=file,filename="$DISK_IMAGE",node-name=hd0 \
    -device virtio-blk-device,drive=hd0 \
    >"$LOG_FILE" 2>&1 &
  QEMU_PID=$!
}

# Match whole lines so the marker inside an echoed command line cannot count.
# The serial console may put a carriage return at either end of a line.
userspace_is_ready() {
  grep -Eq $'^\r?'"${SMOKE_MARKER}"$'\r?$' "$LOG_FILE" &&
    grep -Eq $'^\r?Linux riscv64\r?$' "$LOG_FILE"
}

boot_has_failed() {
  grep -Eq "$BOOT_FAILURE_PATTERN" "$LOG_FILE"
}

report_boot_failure() {
  echo "zCore failed to boot:" >&2
  grep -E --max-count=5 "$BOOT_FAILURE_PATTERN" "$LOG_FILE" >&2 || true
  tail -n 120 "$LOG_FILE" || true
}

report_early_exit() {
  local qemu_exit
  set +e
  wait "$QEMU_PID"
  qemu_exit=$?
  set -e

  echo "QEMU exited before zCore reached userspace (exit=${qemu_exit})" >&2
  tail -n 120 "$LOG_FILE" || true
}

wait_for_userspace() {
  local elapsed
  for ((elapsed = 0; elapsed < BOOT_TIMEOUT_SECS; elapsed++)); do
    # An observed boot error must not be hidden by a userspace marker.
    if boot_has_failed; then
      report_boot_failure
      return 1
    fi
    if userspace_is_ready && ! boot_has_failed; then
      return 0
    fi
    if ! kill -0 "$QEMU_PID" 2>/dev/null; then
      if userspace_is_ready && ! boot_has_failed; then
        return 0
      fi
      report_early_exit
      return 1
    fi
    sleep 1
  done

  if userspace_is_ready && ! boot_has_failed; then
    return 0
  fi

  echo "zCore did not reach userspace within ${BOOT_TIMEOUT_SECS}s" >&2
  tail -n 120 "$LOG_FILE" || true
  return 1
}

main() {
  trap stop_qemu EXIT

  check_prerequisites
  prepare_zcore
  case "$BOOT_MODE" in
    u-boot)
      build_uboot
      make_boot_disk
      start_qemu_uboot
      ;;
    sbi)
      start_qemu_sbi
      ;;
  esac
  wait_for_userspace

  echo "RustSBI booted zCore ${ZCORE_COMMIT:0:8} to userspace successfully (${BOOT_MODE})"
  echo "QEMU log: ${LOG_FILE}"
}

main "$@"
