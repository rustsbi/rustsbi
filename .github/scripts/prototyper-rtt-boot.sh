#!/usr/bin/env bash
set -euo pipefail

# Boot upstream RT-Thread (qemu-virt64-riscv, S-mode) through RustSBI Prototyper
# in QEMU. Two boot paths, selected by the first argument:
#
#   sbi     QEMU -> RustSBI (payload mode) -> RT-Thread at 0x80200000
#   u-boot  QEMU -> u-boot-spl -> RustSBI -> U-Boot -> RT-Thread
#           The SPL embeds RustSBI Prototyper as its SBI firmware; U-Boot
#           loads a legacy uImage of RT-Thread from a FAT disk and starts it
#           with `bootm`.
#
# RT-Thread's qemu-virt64 BSP is an S-mode-under-SBI port: `_start` reads the
# hart id from a0 (startup_gcc.S) and pokes MMIO devices directly, so it needs
# no device tree, which is what makes the legacy uImage bootm path work.

readonly RTT_VERSION=05badce954fc77fc2eb3a41ca785d3a35627ffde
readonly RTT_URL=https://github.com/RT-Thread/rt-thread.git
readonly CACHE_DIR="${RTT_CACHE_DIR:-.cache/rtt}"
readonly WORK_DIR="${RTT_WORK_DIR:-.rtt/work}"
readonly LOG_DIR="${QEMU_LOG_DIR:-qemu-logs}"

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

readonly RTT_SRC="${CACHE_DIR}/rt-thread-${RTT_VERSION}"
readonly RTT_BSP="${RTT_SRC}/bsp/qemu-virt64-riscv"
readonly RTT_BIN="${RTT_BSP}/rtthread.bin"
readonly BIOS_BIN="target/riscv64gc-unknown-none-elf/release/rustsbi-prototyper-payload.bin"
readonly DYNAMIC_ELF="target/riscv64gc-unknown-none-elf/release/rustsbi-prototyper-dynamic.elf"
readonly LOG_FILE="${LOG_DIR}/prototyper-rtt-${BOOT_MODE}.log"
readonly BOOT_TIMEOUT_SECS="${RTT_BOOT_TIMEOUT_SECS:-180}"
readonly JOBS="${RTT_JOBS:-$(nproc)}"
readonly RTT_PREFIX="${RTT_CC_PREFIX:-riscv64-unknown-elf-}"

# Console markers for a successful boot: the RT-Thread banner and the msh shell
# prompt. RT-Thread's qemu-virt64 BSP needs no extra patch to reach either.
readonly SMOKE_MARKERS=("Thread Operating System" "msh")

# Text that means the boot already went wrong: a bootm failure line, U-Boot's
# trap handler, or a RustSBI panic. Without this the job would sit out the
# whole timeout after an early failure. Checked before the success markers so
# an observed error cannot be hidden by a later marker line.
readonly BOOT_FAILURE_PATTERN='panicked at|Unhandled exception|### ERROR ###|System shutdown scheduled due to RustSBI panic'

# U-Boot, pinned like the RT-Thread checkout so the boot test is reproducible.
readonly UB_VERSION="2024.04"
readonly UB_URL="https://github.com/u-boot/u-boot/archive/refs/tags/v${UB_VERSION}.tar.gz"
readonly UB_SHA256="d6b57ce574a0a0504a5b6596644ceacb7f77bde9353779bcf2fde07c4b9a2b92"
readonly CROSS_COMPILE="riscv64-linux-gnu-"

# The sbi path boots the dynamic firmware directly; the u-boot path embeds the
# flattened binary in the SPL.
if [[ "$BOOT_MODE" = u-boot ]]; then
  readonly RUSTSBI_BIN="target/riscv64gc-unknown-none-elf/release/rustsbi-prototyper-dynamic.bin"
else
  readonly RUSTSBI_BIN=""
fi

# The sbi path boots against a blank raw disk the kernel's virtio-blk driver
# can probe; the u-boot path boots a FAT disk carrying the uImage.
readonly DISK_IMAGE="${WORK_DIR}/sd.bin"
readonly UBOOT_DISK_IMAGE="${WORK_DIR}/rtt-boot.img"
readonly RTT_UIMAGE="${WORK_DIR}/rtthread.uimg"

UB_TREE=""
UB_SPL=""
UB_ITB=""

readonly DOWNLOAD_CONNECT_TIMEOUT_SECS="${RTT_DOWNLOAD_CONNECT_TIMEOUT_SECS:-30}"
readonly DOWNLOAD_TIMEOUT_SECS="${RTT_DOWNLOAD_TIMEOUT_SECS:-900}"

QEMU_PID=""

cleanup() {
  if [[ -n "$QEMU_PID" ]] && kill -0 "$QEMU_PID" 2>/dev/null; then
    kill "$QEMU_PID" 2>/dev/null || true
    wait "$QEMU_PID" 2>/dev/null || true
  fi
}
trap cleanup EXIT

download_asset() {
  local url=$1 destination=$2 digest=$3 temp
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
    --retry 3 --retry-all-errors \
    --output "$temp" "$url"; then
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

check_prerequisites() {
  test -s "$DYNAMIC_ELF" || {
    echo "run 'cargo prototyper build' first" >&2
    return 1
  }
  command -v qemu-system-riscv64 >/dev/null || {
    echo "install qemu (qemu-system-misc on Debian/Ubuntu)" >&2
    return 1
  }
  command -v scons >/dev/null || {
    echo "install scons" >&2
    return 1
  }
  if [[ -n "${RTT_EXEC_PATH:-}" ]]; then
    test -x "${RTT_EXEC_PATH%/}/${RTT_PREFIX}gcc" || {
      echo "set RTT_EXEC_PATH to a newlib riscv64 toolchain bin dir" >&2
      return 1
    }
  else
    command -v "${RTT_PREFIX}gcc" >/dev/null || {
      echo "install a newlib riscv64 toolchain (apt gcc-riscv64-unknown-elf has no libc)" >&2
      return 1
    }
  fi
  if [[ "$BOOT_MODE" = u-boot ]]; then
    command -v "${CROSS_COMPILE}gcc" >/dev/null || {
      echo "u-boot mode needs ${CROSS_COMPILE}gcc (apt gcc-riscv64-linux-gnu)" >&2
      return 1
    }
    command -v mkfs.vfat >/dev/null || {
      echo "u-boot mode needs dosfstools" >&2
      return 1
    }
    command -v mcopy >/dev/null || {
      echo "u-boot mode needs mtools" >&2
      return 1
    }
  fi
}

checkout_rtthread() {
  mkdir -p "$CACHE_DIR"
  if [[ ! -d "${RTT_SRC}/.git" ]]; then
    git init --quiet "$RTT_SRC"
    git -C "$RTT_SRC" remote add origin "$RTT_URL"
    git -C "$RTT_SRC" fetch --quiet --depth=1 origin "$RTT_VERSION"
    git -C "$RTT_SRC" checkout --quiet --detach FETCH_HEAD
  fi
  [[ "$(git -C "$RTT_SRC" rev-parse HEAD)" == "$RTT_VERSION" ]] || {
    echo "RT-Thread checkout mismatch" >&2
    return 1
  }
}

build_rtthread() {
  local root exec_path
  root="$(cd "$RTT_SRC" && pwd)"
  if [[ -n "${RTT_EXEC_PATH:-}" ]]; then
    exec_path="${RTT_EXEC_PATH%/}"
  else
    exec_path="$(dirname "$(command -v "${RTT_PREFIX}gcc")")"
  fi
  (cd "$RTT_BSP" && RTT_ROOT="$root" RTT_EXEC_PATH="$exec_path" scons -j"$JOBS")
  test -s "$RTT_BIN"
}

# Rebuild the dynamic firmware linked at the address where the U-Boot SPL
# loads its SBI firmware (CONFIG_SPL_OPENSBI_LOAD_ADDR, 0x80100000 on the
# qemu-riscv64_spl board). The default dynamic build links at 0x80000000 and
# applies its relocations against that constant, so running the same binary
# from 0x80100000 silently faults.
build_sbi_firmware() {
  local config="${WORK_DIR}/sbi-firmware.toml"
  mkdir -p "$WORK_DIR"
  cat >"$config" <<'TOML'
heap_size = 0x15000
page_size = 4096
log_level = "INFO"
link_start_address = 0x80100000
payload_address = 0x80200000
jump_address = 0x80200000
tlb_flush_limit = 16384

[[next_addr]]
start = 0x20000000
end = 0x24000000

[[next_addr]]
start = 0x80000000
end = 0x90000000
TOML
  cargo prototyper build -c "$config"
  test -s "$RUSTSBI_BIN"
}

# Build U-Boot with the RustSBI firmware embedded as its SBI firmware. Not
# cached: the SPL embeds the firmware under test.
build_uboot() {
  local tarball="${WORK_DIR}/u-boot-${UB_VERSION}.tar.gz"
  local tree="${WORK_DIR}/u-boot-${UB_VERSION}"
  local rustsbi_abs
  rustsbi_abs="$(readlink -f "$RUSTSBI_BIN")"

  mkdir -p "$WORK_DIR"
  download_asset "$UB_URL" "$tarball" "$UB_SHA256"

  rm -rf "$tree"
  tar -xzf "$tarball" -C "$WORK_DIR"

  make -C "$tree" ARCH=riscv CROSS_COMPILE="$CROSS_COMPILE" \
    OPENSBI="$rustsbi_abs" qemu-riscv64_spl_defconfig

  # RT-Thread boots from a0 (hart id) and pokes MMIO devices directly, so it
  # needs no device tree contents, but bootm still requires an FDT argument:
  # move U-Boot's control FDT to fdt_addr_r and pass it as the third bootm
  # operand (`-` leaves the ramdisk slot empty).
  "${tree}/scripts/config" --file "${tree}/.config" --enable USE_BOOTCOMMAND
  "${tree}/scripts/config" --file "${tree}/.config" --set-str BOOTCOMMAND \
    "load virtio 0 \${kernel_addr_r} rtthread.uimg; fdt addr \${fdtcontroladdr}; fdt move \${fdtcontroladdr} \${fdt_addr_r} 0x10000; bootm \${kernel_addr_r} - \${fdt_addr_r}"

  # The SPL has to fit the whole u-boot.itb (which embeds RustSBI) in its
  # malloc pool before it can hand it to the next stage; bump the default 1MB
  # so it no longer fails with "Could not get FIT buffer".
  "${tree}/scripts/config" --file "${tree}/.config" --set-val SPL_SYS_MALLOC_SIZE 0x800000

  make -C "$tree" ARCH=riscv CROSS_COMPILE="$CROSS_COMPILE" \
    OPENSBI="$rustsbi_abs" -j"$JOBS"

  UB_TREE="$tree"
  UB_SPL="${tree}/spl/u-boot-spl"
  UB_ITB="${tree}/u-boot.itb"
}

# Wrap the raw RT-Thread binary in a legacy uImage telling `bootm` where to
# copy it and that it is a bare S-mode kernel entered at 0x80200000.
make_boot_disk() {
  rm -f "$RTT_UIMAGE" "$UBOOT_DISK_IMAGE"
  "${UB_TREE}/tools/mkimage" -A riscv -O linux -T kernel -C none \
    -a 0x80200000 -e 0x80200000 -n RT-Thread -d "$RTT_BIN" "$RTT_UIMAGE"

  truncate -s 64M "$UBOOT_DISK_IMAGE"
  mkfs.vfat -F 32 "$UBOOT_DISK_IMAGE" >/dev/null
  mcopy -i "$UBOOT_DISK_IMAGE" "$RTT_UIMAGE" ::rtthread.uimg
}

start_qemu_sbi() {
  mkdir -p "$LOG_DIR"
  : >"$LOG_FILE"
  qemu-system-riscv64 -machine virt -smp 1 -m 256M -nographic -no-reboot \
    -bios "$BIOS_BIN" \
    -drive if=none,file="$DISK_IMAGE",format=raw,id=blk0 \
    -device virtio-blk-device,drive=blk0,bus=virtio-mmio-bus.0 \
    -netdev user,id=n0 \
    -device virtio-net-device,netdev=n0,bus=virtio-mmio-bus.1 \
    >"$LOG_FILE" 2>&1 &
  QEMU_PID=$!
}

start_qemu_uboot() {
  mkdir -p "$LOG_DIR"
  : >"$LOG_FILE"
  qemu-system-riscv64 -machine virt -smp 2 -m 2G -nographic -no-reboot \
    -bios "$UB_SPL" \
    -device loader,file="$UB_ITB",addr=0x80200000 \
    -blockdev driver=file,filename="$UBOOT_DISK_IMAGE",node-name=hd0 \
    -device virtio-blk-device,drive=hd0 \
    >"$LOG_FILE" 2>&1 &
  QEMU_PID=$!
}

markers_seen() {
  local marker
  for marker in "${SMOKE_MARKERS[@]}"; do
    grep -Fq "$marker" "$LOG_FILE" || return 1
  done
}

boot_has_failed() {
  grep -Eq "$BOOT_FAILURE_PATTERN" "$LOG_FILE"
}

report_boot_failure() {
  echo "RT-Thread boot failed:" >&2
  grep -E --max-count=5 "$BOOT_FAILURE_PATTERN" "$LOG_FILE" >&2 || true
  tail -n 120 "$LOG_FILE" || true
}

wait_for_rtthread() {
  local elapsed
  for ((elapsed = 0; elapsed < BOOT_TIMEOUT_SECS; elapsed++)); do
    if boot_has_failed; then
      report_boot_failure
      return 1
    fi
    if markers_seen; then
      return 0
    fi
    if ! kill -0 "$QEMU_PID" 2>/dev/null && markers_seen; then
      return 0
    fi
    sleep 1
  done
  echo "RT-Thread did not boot to the msh prompt within ${BOOT_TIMEOUT_SECS}s" >&2
  tail -n 120 "$LOG_FILE" || true
  return 1
}

main() {
  check_prerequisites
  checkout_rtthread
  build_rtthread
  case "$BOOT_MODE" in
    u-boot)
      build_sbi_firmware
      build_uboot
      make_boot_disk
      start_qemu_uboot
      ;;
    sbi)
      make_sbi_disk
      build_payload_firmware
      start_qemu_sbi
      ;;
  esac
  wait_for_rtthread
  echo "RustSBI booted RT-Thread ${RTT_VERSION} (${BOOT_MODE}): banner and msh prompt seen"
  echo "QEMU log: ${LOG_FILE}"
}

# sbi path helpers, kept at the end for readability.
make_sbi_disk() {
  mkdir -p "$WORK_DIR"
  [[ -f "$DISK_IMAGE" ]] || dd if=/dev/zero of="$DISK_IMAGE" bs=1024 count=65536 status=none
}

build_payload_firmware() {
  cargo prototyper build payload "$RTT_BIN"
  test -s "$BIOS_BIN"
}

main "$@"
