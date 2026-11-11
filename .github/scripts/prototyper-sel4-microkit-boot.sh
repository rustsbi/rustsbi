#!/usr/bin/env bash
# Boot a seL4 Microkit protection domain through the RustSBI Prototyper in QEMU.
#
# Boot chain (verified end to end):
#   QEMU virt -> RustSBI jump firmware (M-mode)
#     -> Microkit loader at 0x90000000 (S-mode)
#     -> seL4 kernel at 0x80200000 -> Monitor -> hello PD (UART output)
#
# The Microkit RISC-V loader links at 0x90000000 (the RISC-V uImage
# convention), so the firmware must be built with a config that jumps there
# instead of the default OpenSBI fw_jump address 0x80200000.
#
# Expects the repository to be the working directory and the firmware to be
# built already: cargo prototyper build produces target/ artifacts, but the
# jump firmware needs a custom config, so this script builds it.
set -euo pipefail

# Pinned Microkit SDK. The sha256 is verified on every run so a corrupted or
# substituted download fails the job instead of silently changing the guest.
readonly MICROKIT_VERSION="2.4.0"
readonly MICROKIT_SDK_URL="https://github.com/seL4/microkit/releases/download/${MICROKIT_VERSION}/microkit-sdk-${MICROKIT_VERSION}-linux-x86-64.tar.gz"
readonly MICROKIT_SDK_SHA256="10bc305713965a01e52b6c7567cea4952b0cc4dde4befa2e6e72b8dce2715f6b"

readonly BOARD="qemu_virt_riscv64"
readonly BOARD_CONFIG="debug"
# The Microkit RISC-V loader link address; the firmware jump target must match.
readonly LOADER_ADDR="0x90000000"

readonly SDK_DIR="target/sel4-microkit/microkit-sdk-${MICROKIT_VERSION}"
readonly PD_DIR=".github/sel4-microkit"
readonly WORK_DIR="target/sel4-microkit/work"
readonly FIRMWARE_BIN="target/riscv64gc-unknown-none-elf/release/rustsbi-prototyper-jump.bin"
readonly LOADER_IMG="${WORK_DIR}/loader.img"
readonly LOG_DIR="${QEMU_LOG_DIR:-qemu-logs}"
readonly LOG_FILE="${LOG_DIR}/prototyper-sel4-microkit.log"
readonly BOOT_TIMEOUT_SECS="${MICROKIT_BOOT_TIMEOUT_SECS:-120}"

# The marker is printed by the hello PD through its mapped 16550 UART.
readonly SUCCESS_MARKER='[hello] Microkit PD started'
# A panic or VM fault halts the system while QEMU stays alive, so without
# these a boot failure would sit out the whole timeout. `faulting PD` is the
# Monitor's wording for a VM fault in a protection domain.
readonly BOOT_FAILURE_PATTERN='MON\|ERROR|faulting PD|Bootstrapping failed|unhandled machine trap|Kernel panic|SBI panic'

fetch_sdk() {
  local tarball="${WORK_DIR}/microkit-sdk-${MICROKIT_VERSION}.tar.gz"

  if [[ -x "${SDK_DIR}/bin/microkit" ]]; then
    echo "Using extracted Microkit SDK ${MICROKIT_VERSION}" >&2
    return
  fi

  mkdir -p "$WORK_DIR"

  if [[ -f "$tarball" ]] && printf '%s  %s\n' "$MICROKIT_SDK_SHA256" "$tarball" | sha256sum --check --status; then
    echo "Using downloaded Microkit SDK tarball" >&2
  else
    echo "Downloading ${MICROKIT_SDK_URL}" >&2
    curl --fail --location \
      --connect-timeout 30 \
      --max-time 600 \
      --retry 3 \
      --retry-all-errors \
      --retry-max-time 600 \
      --output "$tarball" \
      "$MICROKIT_SDK_URL"
  fi

  if ! printf '%s  %s\n' "$MICROKIT_SDK_SHA256" "$tarball" | sha256sum --check --status; then
    echo "Checksum mismatch for ${MICROKIT_SDK_URL}" >&2
    return 1
  fi

  rm -rf "${SDK_DIR}"
  mkdir -p "$(dirname "$SDK_DIR")"
  tar -xzf "$tarball" -C "$(dirname "$SDK_DIR")"
  test -x "${SDK_DIR}/bin/microkit"
}

build_firmware() {
  local config="${WORK_DIR}/qemu-virt-microkit.toml"

  if [[ -s "$FIRMWARE_BIN" ]]; then
    echo "Using built jump firmware" >&2
    return
  fi

  # The default config jumps to the OpenSBI fw_jump address 0x80200000; the
  # Microkit loader expects the next stage at 0x90000000.
  sed -e "s/jump_address = 0x80200000/jump_address = ${LOADER_ADDR}/" \
      -e "s/payload_address = 0x80200000/payload_address = ${LOADER_ADDR}/" \
      firmware/prototyper/config/default.toml > "$config"

  cargo prototyper build --config-file "$config" jump
  test -s "$FIRMWARE_BIN"
}

build_pd() {
  local board="${SDK_DIR}/board/${BOARD}/${BOARD_CONFIG}"
  local elf="${WORK_DIR}/hello.elf"

  mkdir -p "$WORK_DIR"

  # seL4 user space cannot issue ecall-based SBI calls (ecall enters seL4
  # itself), so the PD prints through its MMIO-mapped 16550 UART instead.
  clang --target=riscv64-unknown-elf -march=rv64gc -mabi=lp64d \
    -ffreestanding -nostdlib -O2 \
    -I"${board}/include" \
    -c "${PD_DIR}/hello.c" -o "${WORK_DIR}/hello.o"

  # Microkit 1.3.0+ officially supports linking PDs with LLD; no riscv gcc
  # toolchain is needed. `-z notext` allows relocations in the text segment,
  # which the Microkit ELF loader expects.
  clang --target=riscv64-unknown-elf -march=rv64gc -mabi=lp64d \
    -fuse-ld=lld -nostdlib -Wl,-z,notext \
    "${WORK_DIR}/hello.o" "${board}/lib/libmicrokit.a" -o "$elf"

  # `--image-type` takes binary|elf|uimage (not raw), and the tool only
  # accepts long `--output`/`--report` options.
  "${SDK_DIR}/bin/microkit" \
    --board "$BOARD" --config "$BOARD_CONFIG" --search-path "$WORK_DIR" \
    --image-type binary --output "$LOADER_IMG" --report "${WORK_DIR}/report.txt" \
    "${PD_DIR}/hello.system"

  test -s "$LOADER_IMG"
}

boot_microkit() {
  mkdir -p "$LOG_DIR"
  local status=0

  # The loader image is a raw binary placed at its link address with
  # `-device loader`; `-kernel` would only work for uImage headers.
  timeout "${BOOT_TIMEOUT_SECS}s" qemu-system-riscv64 \
    -machine virt -nographic -m 2G -smp 1 -no-reboot \
    -bios "$FIRMWARE_BIN" \
    -device "loader,file=${LOADER_IMG},addr=${LOADER_ADDR},force-raw=on" \
    >"$LOG_FILE" 2>&1 || status=$?

  if (( status != 0 )) && (( status != 124 )); then
    echo "QEMU exited with status $status; last 120 log lines:" >&2
    tail -n 120 "$LOG_FILE" >&2
    exit 1
  fi

  if ! grep -Fq -- "$SUCCESS_MARKER" "$LOG_FILE"; then
    echo "missing expected output: $SUCCESS_MARKER" >&2
    tail -n 120 "$LOG_FILE" >&2
    exit 1
  fi

  if grep -Eq -- "$BOOT_FAILURE_PATTERN" "$LOG_FILE"; then
    echo "forbidden output in boot log" >&2
    tail -n 120 "$LOG_FILE" >&2
    exit 1
  fi

  echo "RustSBI -> Microkit loader -> seL4 -> hello PD boot passed; log: $LOG_FILE"
}

fetch_sdk
build_firmware
build_pd
boot_microkit
