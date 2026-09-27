#!/usr/bin/env bash
#
# Boot NuttX (rv-virt:flats64) through RustSBI Prototyper in QEMU.
#
# Usage: $0 [bare|u-boot]
#
# Boot paths, both ending in NuttX's NSH shell running the hello example:
#
#   bare    QEMU -> RustSBI (dynamic) -> the NuttX ELF directly.
#   u-boot  QEMU -> RustSBI (dynamic) -> U-Boot (S-mode) -> NuttX (bootm).
#           NuttX is wrapped as a U-Boot legacy image (uImage, load and entry
#           0x80200000) on a small FAT disk that U-Boot loads it from.
#
# NuttX and NuttX-apps are checked out at exact commits; U-Boot is built from
# a pinned release tarball whose digest is verified on every download, with
# the boot command compiled in so no console interaction is needed.
#
# The success condition is the NSH prompt appearing and the built-in hello
# example answering "Hello, World!!": that means RustSBI (and U-Boot, on the
# second path) handed over to NuttX, the kernel came up on the device tree
# QEMU and RustSBI gave it, and its interactive userspace shell works.
#
# Nobody types on the serial console in CI, so the script feeds "hello" to
# QEMU's stdin itself, but only once the NSH prompt shows in the log: earlier
# input could abort U-Boot's autoboot countdown or be dropped before the
# NuttX console opens.
#
# Requires: `cargo prototyper build` to have produced the firmware, plus git,
# curl, a riscv64-unknown-elf toolchain, kconfig-frontends, bison, flex,
# gperf and libncurses for the NuttX build. The u-boot path additionally
# needs a riscv64-linux-gnu cross compiler, u-boot-tools (mkimage),
# dosfstools and mtools.

set -euo pipefail

if (( $# > 1 )); then
  echo "Usage: $0 [bare|u-boot]" >&2
  exit 2
fi

readonly BOOT_MODE="${1:-bare}"
case "$BOOT_MODE" in
  bare | u-boot) ;;
  *)
    echo "Usage: $0 [bare|u-boot]" >&2
    exit 2
    ;;
esac

readonly NUTTX_VERSION="63208908"
readonly NUTTX_COMMIT="63208908ac657ae823b4072fcc4cd255ef8c82a2"
readonly NUTTX_URL="https://github.com/apache/nuttx.git"
readonly APPS_VERSION="c78e4d6"
readonly APPS_COMMIT="c78e4d69eadf15376b05877db36bd2f63dc3b8a6"
readonly APPS_URL="https://github.com/apache/nuttx-apps.git"

readonly UB_VERSION="2024.04"
readonly UB_URL="https://github.com/u-boot/u-boot/archive/refs/tags/v${UB_VERSION}.tar.gz"
readonly UB_SHA256="d6b57ce574a0a0504a5b6596644ceacb7f77bde9353779bcf2fde07c4b9a2b92"

readonly NUTTX_CROSS_COMPILE="riscv64-unknown-elf-"
readonly UB_CROSS_COMPILE="riscv64-linux-gnu-"

# NSH answers the hello example with this line; seeing it means interactive
# userspace works, not merely that the kernel started.
readonly HELLO_GATE='nsh>'
readonly HELLO_MARKER='Hello, World!!'

# Each path must also prove that NuttX really came through that path, not
# merely reached the same shell by another route.
SUCCESS_MARKERS=("$HELLO_GATE" "$HELLO_MARKER")
if [[ "$BOOT_MODE" = u-boot ]]; then
  SUCCESS_MARKERS+=("U-Boot ${UB_VERSION}" "Starting kernel ...")
fi
readonly SUCCESS_MARKERS

# Text that means the boot already went wrong. A panic halts the machine
# while QEMU stays alive, so without this the job would sit out the whole
# timeout. The FDT messages are U-Boot rejecting the device tree; the rest
# are NuttX assertions and U-Boot image handling errors.
readonly BOOT_FAILURE_PATTERN='PANIC|Kernel panic|not syncing|Assertion failed|Failed to reserve memory for fdt|FDT creation failed|System shutdown scheduled due to RustSBI panic|Wrong Image Format|can.t get kernel image'

readonly CACHE_DIR="${NUTTX_CACHE_DIR:-.cache/nuttx}"
readonly WORK_DIR="${NUTTX_WORK_DIR:-.nuttx/work}"
readonly LOG_DIR="${QEMU_LOG_DIR:-qemu-logs}"
readonly LOG_FILE="${LOG_DIR}/prototyper-nuttx-${BOOT_MODE}.log"

readonly RUSTSBI="${NUTTX_RUSTSBI:-target/riscv64gc-unknown-none-elf/release/rustsbi-prototyper-dynamic.bin}"
if [[ "$BOOT_MODE" = bare ]]; then
  readonly BOOT_TIMEOUT_SECS="${NUTTX_BOOT_TIMEOUT_SECS:-180}"
else
  readonly BOOT_TIMEOUT_SECS="${NUTTX_BOOT_TIMEOUT_SECS:-240}"
fi

readonly NUTTX_CACHE="${CACHE_DIR}/nuttx-${NUTTX_VERSION}-${APPS_VERSION}"
readonly NUTTX_ELF="${NUTTX_CACHE}/nuttx"
readonly NUTTX_BIN="${WORK_DIR}/nuttx.bin"
readonly NUTTX_UIMG="${WORK_DIR}/nuttx.uimg"
readonly BOOT_DISK="${WORK_DIR}/${BOOT_MODE}-boot.img"

# U-Boot's build is cached: the boot command is compiled in, so the workflow
# keys that cache on the pinned version and this script.
readonly UB_CACHE="${CACHE_DIR}/u-boot-${UB_VERSION}-smode"
readonly UB_BIN="${UB_CACHE}/u-boot.bin"

readonly QEMU="${NUTTX_QEMU:-qemu-system-riscv64}"

QEMU_PID=""

# Check out an exact commit without keeping remote history, and refuse to
# build anything that is not the pinned commit.
git_checkout() {
  local url=$1
  local dir=$2
  local commit=$3
  local head

  rm -rf "$dir"
  mkdir -p "$dir"
  git init --quiet "$dir"
  git -C "$dir" remote add origin "$url"
  git -C "$dir" fetch --quiet --depth=1 origin "$commit"
  git -C "$dir" checkout --quiet --detach FETCH_HEAD
  head=$(git -C "$dir" rev-parse HEAD)
  if [[ "$head" != "$commit" ]]; then
    echo "Checkout mismatch for $url: expected ${commit}, got ${head}" >&2
    return 1
  fi
}

# Build the rv-virt:flats64 NuttX ELF from the pinned commits. NuttX-apps
# must sit next to the NuttX tree as "apps" for the in-tree hello example.
# Only the ELF is cached; the source trees are rebuilt when it is missing.
prepare_nuttx() {
  local source="${WORK_DIR}/nuttx-src"

  if [[ -s "$NUTTX_ELF" ]]; then
    echo "Using cached NuttX ${NUTTX_VERSION} (${APPS_VERSION} apps)" >&2
    return
  fi

  mkdir -p "$WORK_DIR" "$NUTTX_CACHE"
  git_checkout "$NUTTX_URL" "${source}/nuttx" "$NUTTX_COMMIT"
  git_checkout "$APPS_URL" "${source}/apps" "$APPS_COMMIT"

  (
    cd "${source}/nuttx"
    ./tools/configure.sh rv-virt:flats64
    make -j"$(nproc)"
  )

  test -s "${source}/nuttx/nuttx"
  cp "${source}/nuttx/nuttx" "$NUTTX_ELF"
}

# Fetch a pinned file, verifying its digest on every run so a corrupted,
# substituted or rebuilt download fails the job instead of silently changing
# the test.
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
    --connect-timeout 30 --max-time 1800 \
    --retry 5 --retry-all-errors --retry-max-time 1800 \
    --continue-at - \
    --output "$temp" \
    "$url"; then
    rm -f "$temp"
    return 1
  fi

  if ! printf '%s  %s\n' "$digest" "$temp" | sha256sum --check --status; then
    echo "Checksum mismatch for $url" >&2
    echo "expected ${digest}" >&2
    echo "actual   $(sha256sum "$temp" | cut -d' ' -f1)" >&2
    rm -f "$temp"
    return 1
  fi

  mv "$temp" "$destination"
}

# Build S-mode U-Boot with a compiled-in boot command that loads the NuttX
# uImage from the FAT disk, which QEMU attaches first so that U-Boot numbers
# it virtio 0. RustSBI's patched device tree outgrows the slot U-Boot keeps
# for it, so `bootm` is handed a copy at fdt_addr_r, U-Boot's designated free
# FDT address, with an explicit 64 KiB length bound. The single quotes keep
# the command one hush word and keep the ${...} variables for U-Boot to
# expand at runtime; `scripts/config` rewrites the value with sed, so it
# must not contain `&`.
prepare_uboot() {
  local tarball="${WORK_DIR}/u-boot-${UB_VERSION}.tar.gz"
  local tree="${WORK_DIR}/u-boot-${UB_VERSION}"
  local build="${WORK_DIR}/u-boot-build"

  if [[ -s "$UB_BIN" ]]; then
    echo "Using cached U-Boot ${UB_VERSION}" >&2
    return
  fi

  mkdir -p "$WORK_DIR" "$UB_CACHE"
  download_asset "$UB_URL" "$tarball" "$UB_SHA256"

  rm -rf "$tree" "$build"
  tar -xzf "$tarball" -C "$WORK_DIR"

  make -C "$tree" O="$(realpath -m "$build")" ARCH=riscv CROSS_COMPILE="$UB_CROSS_COMPILE" \
    qemu-riscv64_smode_defconfig
  "${tree}/scripts/config" --file "${build}/.config" --enable USE_BOOTCOMMAND
  # shellcheck disable=SC2016
  "${tree}/scripts/config" --file "${build}/.config" --set-str BOOTCOMMAND \
    'virtio scan; load virtio 0 ${kernel_addr_r} nuttx.uimg; fdt move ${fdtcontroladdr} ${fdt_addr_r} 0x10000; bootm ${kernel_addr_r} - ${fdt_addr_r}'
  make -C "$tree" O="$(realpath -m "$build")" ARCH=riscv CROSS_COMPILE="$UB_CROSS_COMPILE" \
    -j"$(nproc)"

  cp "${build}/u-boot.bin" "$UB_BIN"
}

# U-Boot's bootm boots legacy uImages, so wrap the flat NuttX binary with
# its link address as both load address and entry point.
make_uimage() {
  mkdir -p "$WORK_DIR"
  "${NUTTX_CROSS_COMPILE}objcopy" -O binary "$NUTTX_ELF" "$NUTTX_BIN"
  test -s "$NUTTX_BIN"
  mkimage -A riscv -O linux -T kernel -C none \
    -a 0x80200000 -e 0x80200000 -n NuttX \
    -d "$NUTTX_BIN" "$NUTTX_UIMG" >/dev/null
  test -s "$NUTTX_UIMG"
}

# A whole-disk FAT filesystem written with mtools, so building the disk needs
# neither root nor loop devices. It is rebuilt every run from the freshly
# wrapped uImage.
make_boot_disk() {
  rm -f "$BOOT_DISK"
  truncate -s 64M "$BOOT_DISK"
  mkfs.vfat -F 32 "$BOOT_DISK" >/dev/null
  mcopy -i "$BOOT_DISK" "$NUTTX_UIMG" ::nuttx.uimg
}

check_prerequisites() {
  test -s "$RUSTSBI" || {
    echo "Missing $RUSTSBI; run 'cargo prototyper build' first" >&2
    return 1
  }
  "$QEMU" --version
  if [[ "$BOOT_MODE" = u-boot ]]; then
    command -v mkimage mkfs.vfat mcopy "${NUTTX_CROSS_COMPILE}objcopy" >/dev/null
  fi
}

stop_qemu() {
  if [[ -n "$QEMU_PID" ]] && kill -0 "$QEMU_PID" 2>/dev/null; then
    kill "$QEMU_PID" 2>/dev/null || true
    wait "$QEMU_PID" 2>/dev/null || true
  fi
}

# Type the hello command into the serial console. Wait for the NSH prompt
# first: input sent earlier could stop U-Boot's autoboot countdown or be
# dropped before the NuttX console opens. Both loops are bounded so the
# feeder cannot outlive a failed boot; once QEMU is killed, a write to the
# closed pipe ends it.
feed_console() {
  local i
  for ((i = 0; i < BOOT_TIMEOUT_SECS; i++)); do
    grep -Fq "$HELLO_GATE" "$LOG_FILE" 2>/dev/null && break
    sleep 1
  done
  for ((i = 0; i < 30; i++)); do
    grep -Fq "$HELLO_MARKER" "$LOG_FILE" 2>/dev/null && return
    printf 'hello\r'
    sleep 2
  done
}

start_qemu() {
  local -a boot=()
  local -a disks=()

  case "$BOOT_MODE" in
    bare)
      boot=(-kernel "$NUTTX_ELF")
      ;;
    u-boot)
      boot=(-kernel "$UB_BIN")
      disks=(
        -blockdev "node-name=boot,driver=file,read-only=on,filename=${BOOT_DISK}"
        -device "virtio-blk-device,drive=boot"
      )
      ;;
  esac

  mkdir -p "$LOG_DIR"
  # Truncate any stale log from a previous run before QEMU starts, so the
  # wait loop cannot mistake an old success marker for this boot's.
  : >"$LOG_FILE"
  feed_console | "$QEMU" \
    -machine virt \
    -m 2G \
    -nographic \
    -no-reboot \
    -bios "$RUSTSBI" \
    "${boot[@]}" \
    "${disks[@]}" \
    >"$LOG_FILE" 2>&1 &
  QEMU_PID=$!
}

userspace_is_ready() {
  local marker
  for marker in "${SUCCESS_MARKERS[@]}"; do
    grep -Fq "$marker" "$LOG_FILE" || return 1
  done
}

boot_has_failed() {
  grep -Eq "$BOOT_FAILURE_PATTERN" "$LOG_FILE"
}

report_boot_failure() {
  echo "NuttX failed to boot (${BOOT_MODE}):" >&2
  # The serial log carries terminal control bytes; -a keeps grep printing the
  # matching lines instead of "binary file matches".
  grep -a -E --max-count=5 "$BOOT_FAILURE_PATTERN" "$LOG_FILE" >&2 || true
  tail -n 120 "$LOG_FILE" || true
}

report_early_exit() {
  local qemu_exit
  set +e
  wait "$QEMU_PID"
  qemu_exit=$?
  set -e

  echo "QEMU exited before NuttX reached userspace (exit=${qemu_exit})" >&2
  tail -n 120 "$LOG_FILE" || true
}

wait_for_userspace() {
  local elapsed
  for ((elapsed = 0; elapsed < BOOT_TIMEOUT_SECS; elapsed++)); do
    # Failure is checked first: a boot error after the NSH prompt would still
    # leave every success marker in the log.
    if boot_has_failed; then
      report_boot_failure
      return 1
    fi
    if userspace_is_ready; then
      return 0
    fi
    if ! kill -0 "$QEMU_PID" 2>/dev/null; then
      report_early_exit
      return 1
    fi
    sleep 1
  done

  if ! boot_has_failed && userspace_is_ready; then
    return 0
  fi

  echo "NuttX did not reach userspace within ${BOOT_TIMEOUT_SECS}s" >&2
  tail -n 120 "$LOG_FILE" || true
  return 1
}

main() {
  trap stop_qemu EXIT

  check_prerequisites
  prepare_nuttx
  if [[ "$BOOT_MODE" = u-boot ]]; then
    prepare_uboot
    make_uimage
    make_boot_disk
  fi

  start_qemu
  wait_for_userspace
  stop_qemu

  echo "RustSBI booted NuttX to the NSH shell successfully (${BOOT_MODE})"
  echo "QEMU log: ${LOG_FILE}"
}

main "$@"
