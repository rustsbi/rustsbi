#!/usr/bin/env bash
#
# Boot ArceOS on RustSBI Prototyper in QEMU, either from the dynamic firmware
# (`$0 sbi`, the default) or through U-Boot on the jump firmware (`$0 u-boot`),
# which starts the kernel with `booti`.
#
# Payload and U-Boot are pinned revisions whose build products are cached;
# RustSBI is rebuilt every run and is not.

set -euo pipefail

if (( $# > 1 )); then
  echo "Usage: $0 [sbi|u-boot]" >&2
  exit 2
fi

readonly BOOT_MODE="${1:-sbi}"
case "$BOOT_MODE" in
  sbi | u-boot) ;;
  *)
    echo "Unknown boot path: ${BOOT_MODE}" >&2
    echo "Usage: $0 [sbi|u-boot]" >&2
    exit 2
    ;;
esac

readonly ARCEOS_REPO="https://github.com/arceos-org/arceos"
readonly ARCEOS_REV="8d2e6f97efc4a67359b99db5e519a55b7067b524"
readonly UBOOT_REPO="https://github.com/u-boot/u-boot"
readonly UBOOT_REV="25049ad560826f7dc1c4740883b0016014a59789"
readonly CROSS_COMPILE="riscv64-linux-gnu-"

# U-Boot is loaded where the kernel is linked to run; `booti` reads the payload
# from UBOOT_PAYLOAD_ADDR.
readonly UBOOT_PAYLOAD_ADDR="0x84000000"
readonly UBOOT_LOAD_ADDR="0x80200000"

readonly HELLO_MARKER="Hello, world!"
# axplat logs this on the way out, so both markers together mean a full run.
readonly SHUTDOWN_MARKER="Shutting down"

readonly CACHE_DIR="${ARCEOS_CACHE_DIR:-.cache/arceos}"
readonly UBOOT_CACHE_DIR="${UBOOT_CACHE_DIR:-.cache/uboot-smode}"
# Kept outside the repository: a payload cloned below its root sits in a cargo
# workspace it is not a member of, and cargo refuses to build it.
readonly WORK_DIR="${ARCEOS_WORK_DIR:-${RUNNER_TEMP:-/tmp}/arceos/work}"
readonly LOG_DIR="${QEMU_LOG_DIR:-qemu-logs}"
readonly LOG_FILE="${LOG_DIR}/prototyper-arceos-${BOOT_MODE}.log"
readonly BOOT_TIMEOUT_SECS="${ARCEOS_BOOT_TIMEOUT_SECS:-300}"

readonly ARCEOS_TREE="${WORK_DIR}/arceos"
readonly ARCEOS_BIN="${CACHE_DIR}/helloworld.bin"
readonly ARCEOS_IMAGE="${WORK_DIR}/helloworld.img"

readonly UBOOT_TREE="${WORK_DIR}/u-boot"
readonly UBOOT_BIN="${UBOOT_CACHE_DIR}/u-boot.bin"

if [[ "$BOOT_MODE" = u-boot ]]; then
  readonly RUSTSBI="target/riscv64gc-unknown-none-elf/release/rustsbi-prototyper-jump.elf"
  # `booti` prints this when the image header is wrong, before the kernel runs.
  readonly BOOT_FAILURE_PATTERN="Bad Linux RISCV Image magic"
else
  readonly RUSTSBI="target/riscv64gc-unknown-none-elf/release/rustsbi-prototyper-dynamic.elf"
  readonly BOOT_FAILURE_PATTERN=""
fi

QEMU_PID=""

# Fetch a pinned revision without the rest of the history U-Boot would drag in.
clone_pinned() { # $1=repository $2=revision $3=destination
  local repository=$1
  local revision=$2
  local destination=$3

  if [[ "$(git -C "$destination" rev-parse HEAD 2>/dev/null)" == "$revision" ]]; then
    echo "Using existing $(basename "$destination") checkout" >&2
    return
  fi

  echo "Checking out ${repository} at ${revision:0:12}" >&2
  rm -rf "$destination"
  mkdir -p "$destination"
  git -C "$destination" init --quiet
  git -C "$destination" remote add origin "$repository"
  git -C "$destination" fetch --quiet --depth 1 origin "$revision"
  git -C "$destination" checkout --quiet FETCH_HEAD
}

# The payload's own check installs these with the toolchain it pins
# (nightly-2025-05-20), too old for the cargo-axplat release `cargo install`
# resolves now; installing them with this repository's toolchain lets it pass.
#
# Built from inside the tree, not with `make -C`: the payload derives TARGET_DIR
# and OUT_CONFIG from $(PWD), so `-C` would write into RustSBI's own target
# directory and drop .axconfig.toml at the repository root.
prepare_arceos() {
  if [[ -s "$ARCEOS_BIN" ]]; then
    echo "Using cached ArceOS build" >&2
    return
  fi

  mkdir -p "$CACHE_DIR" "$WORK_DIR"
  clone_pinned "$ARCEOS_REPO" "$ARCEOS_REV" "$ARCEOS_TREE"
  cargo install --locked cargo-axplat axconfig-gen
  (
    cd "$ARCEOS_TREE"
    # LOG=info: the shutdown marker this script waits for is an `info!`.
    make A=examples/helloworld ARCH=riscv64 LOG=info
  )

  cp "${ARCEOS_TREE}/examples/helloworld/helloworld_riscv64-qemu-virt.bin" "$ARCEOS_BIN"
}

# The S-mode defconfig ships no boot command, so one is baked in. The tree it
# passes on is U-Boot's control tree, i.e. the one RustSBI patched for it.
build_uboot() {
  if [[ -s "$UBOOT_BIN" ]]; then
    echo "Using cached U-Boot build" >&2
    return
  fi

  mkdir -p "$UBOOT_CACHE_DIR" "$WORK_DIR"
  clone_pinned "$UBOOT_REPO" "$UBOOT_REV" "$UBOOT_TREE"

  make -C "$UBOOT_TREE" qemu-riscv64_smode_defconfig
  "${UBOOT_TREE}/scripts/config" --file "${UBOOT_TREE}/.config" \
    --enable CONFIG_USE_BOOTCOMMAND \
    --set-str CONFIG_BOOTCOMMAND "booti ${UBOOT_PAYLOAD_ADDR} - \${fdtcontroladdr}"
  make -C "$UBOOT_TREE" -j"$(nproc)" CROSS_COMPILE="$CROSS_COMPILE"

  cp "${UBOOT_TREE}/u-boot.bin" "$UBOOT_BIN"
}

# `booti` needs the 64-byte RISC-V Linux Image header, whose text_offset puts
# the payload at 0x801fffc0 and whose code0 jumps the 64 bytes to 0x80200000.
build_image_header() { # $1=raw kernel binary $2=wrapped image
  mkdir -p "$(dirname "$2")"

  python3 - "$1" "$2" <<'PY'
import struct
import sys

TEXT_OFFSET = 0x1FFFC0
JAL_X0_64 = 0x0400006F
MAGIC = 0x05435352

raw = open(sys.argv[1], "rb").read()
header = struct.pack(
    "<IIQQQIIQQII",
    JAL_X0_64, 0,                # code0, code1
    TEXT_OFFSET, len(raw) + 64,  # text_offset, image_size
    0,                           # flags
    2, 0,                        # version, res1
    0,                           # res2
    0x0000005643534952,          # res3: "RISCV\0\0\0"
    MAGIC, 0,                    # magic, res4
)
assert len(header) == 64, len(header)
open(sys.argv[2], "wb").write(header + raw)
PY
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

start_qemu_sbi() {
  mkdir -p "$LOG_DIR"
  : >"$LOG_FILE"
  qemu-system-riscv64 \
    -machine virt \
    -smp 1 \
    -m 128M \
    -nographic \
    -no-reboot \
    -bios "$RUSTSBI" \
    -kernel "$ARCEOS_BIN" \
    >"$LOG_FILE" 2>&1 &
  QEMU_PID=$!
}

start_qemu_uboot() {
  mkdir -p "$LOG_DIR"
  : >"$LOG_FILE"
  qemu-system-riscv64 \
    -machine virt \
    -smp 1 \
    -m 128M \
    -nographic \
    -no-reboot \
    -bios "$RUSTSBI" \
    -device "loader,file=${UBOOT_BIN},addr=${UBOOT_LOAD_ADDR}" \
    -device "loader,file=${ARCEOS_IMAGE},addr=${UBOOT_PAYLOAD_ADDR}" \
    >"$LOG_FILE" 2>&1 &
  QEMU_PID=$!
}

# The kernel writes NUL bytes into the log, so grep needs -a to print matches.
kernel_is_ready() {
  grep -Faq "$HELLO_MARKER" "$LOG_FILE" && grep -Faq "$SHUTDOWN_MARKER" "$LOG_FILE"
}

boot_has_failed() {
  [[ -n "$BOOT_FAILURE_PATTERN" ]] || return 1
  grep -Eaq "$BOOT_FAILURE_PATTERN" "$LOG_FILE"
}

report_boot_failure() {
  echo "ArceOS failed to boot:" >&2
  grep -Ea --max-count=5 "$BOOT_FAILURE_PATTERN" "$LOG_FILE" >&2 || true
  tail -n 120 "$LOG_FILE" >&2 || true
}

report_early_exit() {
  local qemu_exit
  set +e
  wait "$QEMU_PID"
  qemu_exit=$?
  set -e

  echo "QEMU exited before ArceOS shut the machine down (exit=${qemu_exit})" >&2
  tail -n 120 "$LOG_FILE" >&2 || true
}

wait_for_kernel() {
  local elapsed
  for ((elapsed = 0; elapsed < BOOT_TIMEOUT_SECS; elapsed++)); do
    # Checked first: the kernel may have passed and shut down already.
    if kernel_is_ready; then
      return 0
    fi
    if boot_has_failed; then
      report_boot_failure
      return 1
    fi
    if ! kill -0 "$QEMU_PID" 2>/dev/null; then
      if kernel_is_ready; then
        return 0
      fi
      report_early_exit
      return 1
    fi
    sleep 1
  done

  if kernel_is_ready; then
    return 0
  fi

  echo "ArceOS did not boot within ${BOOT_TIMEOUT_SECS}s" >&2
  tail -n 120 "$LOG_FILE" >&2 || true
  return 1
}

main() {
  trap cleanup EXIT

  check_prerequisites
  prepare_arceos
  case "$BOOT_MODE" in
    u-boot)
      build_uboot
      build_image_header "$ARCEOS_BIN" "$ARCEOS_IMAGE"
      start_qemu_uboot
      ;;
    sbi)
      start_qemu_sbi
      ;;
  esac
  wait_for_kernel

  echo "RustSBI booted ArceOS successfully (${BOOT_MODE})"
  echo "QEMU log: ${LOG_FILE}"
}

main "$@"
