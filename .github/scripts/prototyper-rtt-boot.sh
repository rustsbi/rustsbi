#!/usr/bin/env bash
set -euo pipefail

# Boot upstream RT-Thread (qemu-virt64-riscv, S-mode) through RustSBI Prototyper
# in QEMU. Path: QEMU -> RustSBI (payload mode) -> RT-Thread at 0x80200000.
readonly RTT_VERSION=05badce954fc77fc2eb3a41ca785d3a35627ffde
readonly RTT_URL=https://github.com/RT-Thread/rt-thread.git
readonly CACHE_DIR="${RTT_CACHE_DIR:-.cache/rtt}"
readonly RTT_SRC="${CACHE_DIR}/rt-thread-${RTT_VERSION}"
readonly RTT_BSP="${RTT_SRC}/bsp/qemu-virt64-riscv"
readonly RTT_BIN="${RTT_BSP}/rtthread.bin"
readonly BIOS_BIN="target/riscv64gc-unknown-none-elf/release/rustsbi-prototyper-payload.bin"
readonly DYNAMIC_ELF="target/riscv64gc-unknown-none-elf/release/rustsbi-prototyper-dynamic.elf"
readonly LOG_DIR="${QEMU_LOG_DIR:-qemu-logs}"
readonly LOG_FILE="${LOG_DIR}/prototyper-rtt.log"
readonly DISK_IMAGE="${CACHE_DIR}/sd.bin"
readonly BOOT_TIMEOUT_SECS="${RTT_BOOT_TIMEOUT_SECS:-180}"
readonly JOBS="${RTT_JOBS:-$(nproc)}"
readonly RTT_PREFIX="${RTT_CC_PREFIX:-riscv64-unknown-elf-}"

# Console markers for a successful boot: the RT-Thread banner and the msh shell
# prompt. RT-Thread's qemu-virt64 BSP needs no extra patch to reach either.
readonly SMOKE_MARKERS=("Thread Operating System" "msh")

QEMU_PID=""

cleanup() {
  if [[ -n "$QEMU_PID" ]] && kill -0 "$QEMU_PID" 2>/dev/null; then
    kill "$QEMU_PID" 2>/dev/null || true
    wait "$QEMU_PID" 2>/dev/null || true
  fi
}
trap cleanup EXIT

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
}

# Fetch RT-Thread at an exact commit so the boot test is reproducible, then
# build with scons. The image is cached and reused unless the recipe here
# changes; see the CI cache key which hashes this script.
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
  local root
  root="$(cd "$RTT_SRC" && pwd)"
  local exec_path
  if [[ -n "${RTT_EXEC_PATH:-}" ]]; then
    exec_path="${RTT_EXEC_PATH%/}"
  else
    exec_path="$(dirname "$(command -v "${RTT_PREFIX}gcc")")"
  fi
  (cd "$RTT_BSP" && RTT_ROOT="$root" RTT_EXEC_PATH="$exec_path" scons -j"$JOBS")
  test -s "$RTT_BIN"
}

make_blank_disk() {
  [[ -f "$DISK_IMAGE" ]] || dd if=/dev/zero of="$DISK_IMAGE" bs=1024 count=65536 status=none
}

build_payload_firmware() {
  cargo prototyper build payload "$RTT_BIN"
  test -s "$BIOS_BIN"
}

start_qemu() {
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

markers_seen() {
  local marker
  for marker in "${SMOKE_MARKERS[@]}"; do
    grep -Fq "$marker" "$LOG_FILE" || return 1
  done
}

wait_for_rtthread() {
  local elapsed
  for ((elapsed = 0; elapsed < BOOT_TIMEOUT_SECS; elapsed++)); do
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
  make_blank_disk
  build_payload_firmware
  start_qemu
  wait_for_rtthread
  echo "RustSBI booted RT-Thread ${RTT_VERSION} (payload mode, no U-Boot): banner and msh prompt seen"
  echo "QEMU log: ${LOG_FILE}"
}

main "$@"
