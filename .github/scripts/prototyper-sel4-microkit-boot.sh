#!/usr/bin/env bash
# Boot a seL4 Microkit protection domain through the RustSBI Prototyper in QEMU.
#
# Two boot paths are covered, selected by the first argument:
#
#   direct  QEMU virt -> RustSBI jump firmware (M-mode)
#             -> Microkit loader at 0x90000000 (S-mode)
#             -> seL4 kernel at 0x80200000 -> Monitor -> hello PD (UART output).
#           The loader is placed at its link address with `-device loader`; no
#           bootloader is involved.
#
#   u-boot  QEMU virt -> u-boot-spl (M-mode, RustSBI Prototyper embedded as
#             its OpenSBI payload in a FIT) -> RustSBI -> u-boot.itb at
#             0x80200000 -> U-Boot proper (S-mode) -> FAT virtio disk: load
#             loader.uimg; bootm -> Microkit loader at 0x90000000 -> seL4 kernel
#             at 0x80200000 -> hello PD (UART output).
#           On RISC-V only `bootm` jumps to the next stage with a0=hart ID and
#           a1=device tree, which is what the Microkit loader expects. `bootelf`
#           and `go` pass argc/argv instead and would corrupt the loader's
#           entry, so the loader is packaged as a uImage whose header carries
#           its 0x90000000 load/entry address.
#
# The Microkit RISC-V loader links at 0x90000000 (the RISC-V uImage
# convention), so on the direct path the firmware must be built with a config
# that jumps there instead of the default OpenSBI fw_jump address 0x80200000.
# The u-boot path uses the standard `cargo prototyper build` firmware at the
# default jump address, which is U-Boot's TEXT_BASE, and U-Boot starts the
# loader itself.
#
# Expects the repository to be the working directory and the firmware to be
# built already: cargo prototyper build produces target/ artifacts, but the
# direct path's jump firmware needs a custom config, so this script builds
# that one. The u-boot path needs the U-Boot build toolchain (bison,
# build-essential, flex, gcc-riscv64-linux-gnu, libssl-dev, python3-dev,
# python3-setuptools, swig), with mkfs.vfat from dosfstools and mcopy from
# mtools for the boot disk.
set -euo pipefail

# shellcheck source=.github/scripts/ci-uboot.sh
source "$(dirname "${BASH_SOURCE[0]}")/ci-uboot.sh"

if (( $# > 1 )); then
  echo "Usage: $0 [direct|u-boot]" >&2
  exit 2
fi

readonly BOOT_MODE="${1:-direct}"
case "$BOOT_MODE" in
  direct | u-boot) ;;
  *)
    echo "Unknown boot path: ${BOOT_MODE}" >&2
    echo "Usage: $0 [direct|u-boot]" >&2
    exit 2
    ;;
esac

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
readonly LOG_FILE="${LOG_DIR}/prototyper-sel4-microkit-${BOOT_MODE}.log"
readonly BOOT_TIMEOUT_SECS="${MICROKIT_BOOT_TIMEOUT_SECS:-120}"

# The u-boot path embeds the firmware `cargo prototyper build` produces at the
# default jump address 0x80200000 (U-Boot's TEXT_BASE) as the OpenSBI payload of
# U-Boot's SPL. The 0x90000000 jump firmware above belongs to the direct path
# only.
readonly RUSTSBI="${SEL4_RUSTSBI_BIN:-target/riscv64gc-unknown-none-elf/release/rustsbi-prototyper.bin}"
readonly UB_VERSION="2024.04"
readonly LOADER_UIMAGE="${WORK_DIR}/loader.uimg"
readonly BOOT_DISK="${WORK_DIR}/sel4-boot.img"

# Populated by build_uboot; kept as mutable globals because the tree lives
# under WORK_DIR and only the FIT is repacked every run.
UB_SPL=""
UB_ITB=""

# The marker is printed by the hello PD through its mapped 16550 UART.
readonly SUCCESS_MARKER='[hello] Microkit PD started'
# The u-boot path must also prove that U-Boot ran and started the loader instead
# of Microkit being reached by another route.
UB_SUCCESS_MARKERS=("U-Boot ${UB_VERSION}" "Starting kernel ...")
readonly UB_SUCCESS_MARKERS
# A panic or VM fault halts the system while QEMU stays alive, so without
# these a boot failure would sit out the whole timeout. `faulting PD` is the
# Monitor's wording for a VM fault in a protection domain.
readonly BOOT_FAILURE_PATTERN='MON\|ERROR|faulting PD|Bootstrapping failed|unhandled machine trap|Kernel panic|SBI panic'
# U-Boot's own image and device tree failures, as reported by bootm.
readonly UB_BOOT_FAILURE_PATTERN="Wrong Image Format|Bad Linux RISCV Image magic|Ramdisk image is corrupt|Could not find a valid device tree|FDT creation failed|Failed to reserve memory for fdt"

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
  local image_type output

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

  # The direct path wants the raw loader binary; the u-boot path wants a
  # uImage whose header carries the 0x90000000 load/entry address, because
  # only `bootm` gives the loader the hart ID and device tree registers
  # (a0/a1) it expects. `--image-type` takes binary|elf|uimage (not raw), and
  # the tool only accepts long `--output`/`--report` options.
  if [[ "$BOOT_MODE" = u-boot ]]; then
    image_type=uimage
    output=$LOADER_UIMAGE
  else
    image_type=binary
    output=$LOADER_IMG
  fi

  "${SDK_DIR}/bin/microkit" \
    --board "$BOARD" --config "$BOARD_CONFIG" --search-path "$WORK_DIR" \
    --image-type "$image_type" --output "$output" --report "${WORK_DIR}/report.txt" \
    "${PD_DIR}/hello.system"

  test -s "$output"
}

# Repack the shared U-Boot build with the current RustSBI.
build_uboot() {
  local tree="${WORK_DIR}/u-boot"
  ci_uboot_build 2024.04 spl sel4-spl "$tree"
  ci_uboot_fit "$tree" "$RUSTSBI"
  UB_SPL="$tree/spl/u-boot-spl"
  UB_ITB="$tree/u-boot.itb"
}

# A whole-disk FAT filesystem written with mtools, so building the boot disk
# needs neither root nor loop devices. The loader uImage carries its own load
# address, so no uboot.env or initramfs entries are needed.
make_boot_disk() {
  rm -f "$BOOT_DISK"
  truncate -s 64M "$BOOT_DISK"
  mkfs.vfat -F 32 "$BOOT_DISK" >/dev/null
  mcopy -i "$BOOT_DISK" "$LOADER_UIMAGE" ::loader.uimg
}

boot_microkit() {
  mkdir -p "$LOG_DIR"
  local status=0

  if [[ "$BOOT_MODE" = u-boot ]]; then
    # u-boot-spl is given as `-bios`; the FIT is placed at U-Boot's TEXT_BASE
    # (0x80200000) where the SPL's OpenSBI payload jumps. The boot disk is a
    # virtio block device the SPL-embedded U-Boot proper scans for loader.uimg.
    timeout "${BOOT_TIMEOUT_SECS}s" qemu-system-riscv64 \
      -machine virt -nographic -m 2G -smp 1 -no-reboot \
      -bios "$UB_SPL" \
      -device loader,file="$UB_ITB",addr=0x80200000 \
      -blockdev driver=file,filename="$BOOT_DISK",node-name=hd0 \
      -device virtio-blk-device,drive=hd0 \
      >"$LOG_FILE" 2>&1 || status=$?
  else
    # The loader image is a raw binary placed at its link address with
    # `-device loader`; `-kernel` would only work for uImage headers.
    timeout "${BOOT_TIMEOUT_SECS}s" qemu-system-riscv64 \
      -machine virt -nographic -m 2G -smp 1 -no-reboot \
      -bios "$FIRMWARE_BIN" \
      -device "loader,file=${LOADER_IMG},addr=${LOADER_ADDR},force-raw=on" \
      >"$LOG_FILE" 2>&1 || status=$?
  fi

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

  if [[ "$BOOT_MODE" = u-boot ]]; then
    local marker
    for marker in "${UB_SUCCESS_MARKERS[@]}"; do
      if ! grep -Fq -- "$marker" "$LOG_FILE"; then
        echo "missing expected output: $marker" >&2
        tail -n 120 "$LOG_FILE" >&2
        exit 1
      fi
    done
  fi

  if grep -Eq -- "$BOOT_FAILURE_PATTERN" "$LOG_FILE"; then
    echo "forbidden output in boot log" >&2
    tail -n 120 "$LOG_FILE" >&2
    exit 1
  fi

  if [[ "$BOOT_MODE" = u-boot ]] && grep -Eq -- "$UB_BOOT_FAILURE_PATTERN" "$LOG_FILE"; then
    echo "forbidden U-Boot output in boot log" >&2
    tail -n 120 "$LOG_FILE" >&2
    exit 1
  fi

  echo "RustSBI -> Microkit loader -> seL4 -> hello PD boot passed (${BOOT_MODE}); log: $LOG_FILE"
}

fetch_sdk
if [[ "$BOOT_MODE" = u-boot ]]; then
  # `cargo prototyper build` is run by the workflow; the standard flattened
  # firmware it produces is the SPL's OpenSBI payload on this path.
  test -s "$RUSTSBI" || {
    echo "Missing $RUSTSBI; run 'cargo prototyper build' first" >&2
    exit 1
  }
  build_pd
  build_uboot
  make_boot_disk
else
  build_firmware
  build_pd
fi
boot_microkit
