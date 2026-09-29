#!/usr/bin/env bash
#
# Boot the OpenWrt release through RustSBI Prototyper in QEMU.
#
# The official SiFive Unleashed release is an SD-card image, not a QEMU virt
# image, and its kernel lacks the built-in block drivers needed to mount it on
# virt. To avoid rebuilding OpenWrt, this script extracts its Linux Image and
# ext4 rootfs, then repacks the rootfs as an external initramfs. The source
# image is checksum-pinned; only /init and serial-console setup are adjusted.
#
# The direct path uses QEMU's fw_dynamic handoff. The U-Boot path embeds the
# RustSBI binary built from the commit under test in U-Boot SPL, then loads the
# OpenWrt kernel and initramfs into guest memory. Both paths log in over the
# serial console and run a smoke command that verifies the release and ISA.

set -euo pipefail

if (( $# > 1 )); then
  echo "Usage: $0 [sbi|u-boot]" >&2
  exit 2
fi

# Default to the direct SBI boot path.
readonly BOOT_MODE="${1:-sbi}"
case "$BOOT_MODE" in
  sbi | u-boot) ;;
  *)
    echo "Unknown OpenWrt boot mode: ${BOOT_MODE}" >&2
    echo "Usage: $0 [sbi|u-boot]" >&2
    exit 2
    ;;
esac

readonly OPENWRT_RELEASE="25.12.5"
readonly OPENWRT_IMAGE="openwrt-${OPENWRT_RELEASE}-sifiveu-generic-sifive_unleashed-ext4-sdcard.img.gz"
readonly OPENWRT_BASE_URL="https://downloads.openwrt.org/releases/${OPENWRT_RELEASE}/targets/sifiveu/generic"
readonly OPENWRT_SHA256="41ab77db11d689d3fde501dfd6f9a64f1adc0a53b6b2fb66eb3dabc635e3e0c7"

readonly UBOOT_REPOSITORY="https://github.com/u-boot/u-boot.git"
readonly UBOOT_VERSION="v2026.07"
readonly UBOOT_COMMIT="ece349ade2973e220f524ce59e59711cc919263f"

readonly CACHE_DIR="${OPENWRT_CACHE_DIR:-.cache/openwrt/${OPENWRT_RELEASE}}"
readonly IMAGE_ARCHIVE="${CACHE_DIR}/${OPENWRT_IMAGE}"
readonly UBOOT_CACHE_DIR="${OPENWRT_UBOOT_CACHE_DIR:-.cache/u-boot/${UBOOT_VERSION}}"
readonly UBOOT_BUILD_DIR="${OPENWRT_UBOOT_BUILD_DIR:-target/u-boot-openwrt}"
readonly WORK_DIR="${OPENWRT_WORK_DIR:-.openwrt/work}"
readonly RAW_IMAGE="${WORK_DIR}/openwrt.img"
readonly ROOTFS_IMAGE="${WORK_DIR}/rootfs.ext4"
readonly ROOTFS_DIR="${WORK_DIR}/rootfs"
readonly KERNEL_IMAGE="${WORK_DIR}/Image"
readonly INITRAMFS="${WORK_DIR}/openwrt-initramfs.cpio.gz"

readonly RUSTSBI_ELF="target/riscv64gc-unknown-none-elf/release/rustsbi-prototyper-dynamic.elf"
readonly RUSTSBI_BIN="target/riscv64gc-unknown-none-elf/release/rustsbi-prototyper-dynamic.bin"
readonly LOG_DIR="${QEMU_LOG_DIR:-qemu-logs}"
readonly LOG_FILE="${LOG_DIR}/prototyper-openwrt-${BOOT_MODE}.log"
readonly BOOT_TIMEOUT_SECS="${OPENWRT_BOOT_TIMEOUT_SECS:-180}"
readonly DOWNLOAD_CONNECT_TIMEOUT_SECS="${OPENWRT_DOWNLOAD_CONNECT_TIMEOUT_SECS:-30}"
readonly DOWNLOAD_TIMEOUT_SECS="${OPENWRT_DOWNLOAD_TIMEOUT_SECS:-900}"

readonly KERNEL_LOAD_ADDRESS="0x88000000"
readonly INITRAMFS_LOAD_ADDRESS="0x8a000000"
readonly SMOKE_MARKER="RUSTSBI-OPENWRT-SMOKE-OK ${OPENWRT_RELEASE} riscv64"
readonly SMOKE_COMMAND=". /etc/openwrt_release && test \"\$DISTRIB_RELEASE\" = \"${OPENWRT_RELEASE}\" && test \"\$(uname -m)\" = riscv64 && echo \"RUSTSBI-OPENWRT-SMOKE-OK \$DISTRIB_RELEASE \$(uname -m)\""
readonly BOOT_FAILURE_PATTERN='Kernel panic|not syncing|VFS: Cannot open root device|Attempted to kill init|Bad Linux RISCV Image magic'

QEMU_PID=""
QEMU_INPUT=""
QEMU_INPUT_DIR=""
DOWNLOAD_TEMP=""
UB_SPL=""
UB_ITB=""
INITRAMFS_SIZE_HEX=""

verify_openwrt_image() {
  [[ -f "$IMAGE_ARCHIVE" ]] || return 1
  printf '%s  %s\n' "$OPENWRT_SHA256" "$IMAGE_ARCHIVE" | sha256sum --check
}

download_openwrt_image() {
  mkdir -p "$CACHE_DIR"
  if verify_openwrt_image >/dev/null 2>&1; then
    verify_openwrt_image
    return
  fi

  echo "Downloading OpenWrt ${OPENWRT_RELEASE} image"
  DOWNLOAD_TEMP=$(mktemp "${IMAGE_ARCHIVE}.part.XXXXXX")
  curl --fail --location \
    --connect-timeout "$DOWNLOAD_CONNECT_TIMEOUT_SECS" \
    --max-time "$DOWNLOAD_TIMEOUT_SECS" \
    --retry 3 \
    --retry-all-errors \
    --retry-max-time "$DOWNLOAD_TIMEOUT_SECS" \
    --output "$DOWNLOAD_TEMP" \
    "${OPENWRT_BASE_URL}/${OPENWRT_IMAGE}"
  mv "$DOWNLOAD_TEMP" "$IMAGE_ARCHIVE"
  DOWNLOAD_TEMP=""
  verify_openwrt_image
}

prepare_openwrt_assets() {
  local boot_start root_start root_size sector_size kernel_magic

  rm -rf "$ROOTFS_DIR"
  mkdir -p "$WORK_DIR" "$ROOTFS_DIR"
  gzip --decompress --stdout "$IMAGE_ARCHIVE" >"$RAW_IMAGE"

  # Read the pinned image's partition layout instead of relying on loop
  # devices or root privileges. The disk has four GPT entries; the two
  # payloads needed by this test are the Linux Image in partition 3's FAT
  # filesystem and the ext4 rootfs in partition 4.
  read -r sector_size boot_start root_start root_size < <(
    sfdisk --json "$RAW_IMAGE" | python3 -c '
import json, sys
table = json.load(sys.stdin)["partitiontable"]
parts = table["partitions"]
if table["label"] != "gpt" or len(parts) != 4:
    raise SystemExit("unexpected OpenWrt partition layout")
print(table["sectorsize"], parts[2]["start"], parts[3]["start"], parts[3]["size"])
'
  )

  mcopy -o -i "${RAW_IMAGE}@@$((boot_start * sector_size))" ::Image "${KERNEL_IMAGE}.gz"
  gzip --decompress --stdout "${KERNEL_IMAGE}.gz" >"$KERNEL_IMAGE"
  kernel_magic=$(dd if="$KERNEL_IMAGE" bs=1 skip=48 count=5 status=none)
  if [[ "$kernel_magic" != RISCV ]]; then
    echo "The OpenWrt kernel does not contain the RISC-V Image magic" >&2
    return 1
  fi

  dd if="$RAW_IMAGE" of="$ROOTFS_IMAGE" bs="$sector_size" \
    skip="$root_start" count="$root_size" status=none

  # debugfs may warn that an unprivileged user cannot restore numeric file
  # ownership. cpio sets every archive entry back to root:root below, so those
  # warnings are expected and do not affect the guest filesystem.
  if ! debugfs -R "rdump / ${ROOTFS_DIR}" "$ROOTFS_IMAGE" \
    >"${WORK_DIR}/debugfs.log" 2>&1; then
    tail -n 80 "${WORK_DIR}/debugfs.log" >&2 || true
    return 1
  fi
  test -x "${ROOTFS_DIR}/sbin/init"
  test -x "${ROOTFS_DIR}/bin/busybox"
  grep -Fq "DISTRIB_RELEASE='${OPENWRT_RELEASE}'" "${ROOTFS_DIR}/etc/openwrt_release"

  # The board image normally exposes a SiFive UART. Add QEMU virt's 16550
  # console, then provide /init so Linux keeps the external initramfs as root.
  if ! grep -Fq 'ttyS0::askfirst:' "${ROOTFS_DIR}/etc/inittab"; then
    printf '%s\n' 'ttyS0::askfirst:/usr/libexec/login.sh' >>"${ROOTFS_DIR}/etc/inittab"
  fi
  ln -s sbin/init "${ROOTFS_DIR}/init"

  (
    cd "$ROOTFS_DIR"
    find . -print0 \
      | cpio --null --create --format=newc --owner=0:0 --quiet \
      | gzip -9
  ) >"$INITRAMFS"
  INITRAMFS_SIZE_HEX=$(printf '0x%x' "$(stat -c %s "$INITRAMFS")")

  # Keep the two loader ranges disjoint if a later pinned release grows.
  if (( $(stat -c %s "$KERNEL_IMAGE") >= 0x2000000 )); then
    echo "OpenWrt kernel is too large for the fixed U-Boot load layout" >&2
    return 1
  fi
}

prepare_u_boot_source() {
  if [[ ! -d "${UBOOT_CACHE_DIR}/.git" ]]; then
    if [[ -e "$UBOOT_CACHE_DIR" ]]; then
      echo "U-Boot cache path exists but is not a Git repository: ${UBOOT_CACHE_DIR}" >&2
      return 1
    fi
    mkdir -p "$(dirname "$UBOOT_CACHE_DIR")"
    git clone --filter=blob:none --no-checkout "$UBOOT_REPOSITORY" "$UBOOT_CACHE_DIR"
  fi

  if ! git -C "$UBOOT_CACHE_DIR" cat-file -e "${UBOOT_COMMIT}^{commit}" 2>/dev/null; then
    git -C "$UBOOT_CACHE_DIR" fetch --depth=1 origin "$UBOOT_COMMIT"
  fi
  git -C "$UBOOT_CACHE_DIR" checkout --detach --force "$UBOOT_COMMIT"
  test "$(git -C "$UBOOT_CACHE_DIR" rev-parse HEAD)" = "$UBOOT_COMMIT"
}

build_u_boot() {
  local build_path rustsbi_path
  rustsbi_path=$(realpath "$RUSTSBI_BIN")
  build_path=$(realpath -m "$UBOOT_BUILD_DIR")
  test -s "$rustsbi_path"
  mkdir -p "$build_path"

  make -C "$UBOOT_CACHE_DIR" O="$build_path" \
    ARCH=riscv CROSS_COMPILE=riscv64-linux-gnu- qemu-riscv64_spl_defconfig
  make -C "$UBOOT_CACHE_DIR" O="$build_path" \
    ARCH=riscv CROSS_COMPILE=riscv64-linux-gnu- OPENSBI="$rustsbi_path" \
    -j"$(nproc)"

  UB_SPL="${build_path}/spl/u-boot-spl"
  UB_ITB="${build_path}/u-boot.itb"
  test -s "$UB_SPL"
  test -s "$UB_ITB"
}

check_prerequisites() {
  test -s "$RUSTSBI_ELF" || {
    echo "Missing $RUSTSBI_ELF; run 'cargo prototyper build' first" >&2
    return 1
  }
  test -s "$RUSTSBI_BIN"
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
  if [[ -n "$QEMU_INPUT" ]]; then
    exec 3>&- 3<&- || true
    rm -f "$QEMU_INPUT"
  fi
  if [[ -n "$QEMU_INPUT_DIR" ]]; then
    rmdir "$QEMU_INPUT_DIR" 2>/dev/null || true
  fi
  if [[ -n "$DOWNLOAD_TEMP" ]]; then
    rm -f "$DOWNLOAD_TEMP"
  fi
}

create_qemu_console() {
  mkdir -p "$LOG_DIR"
  : >"$LOG_FILE"
  QEMU_INPUT_DIR=$(mktemp -d "${TMPDIR:-/tmp}/rustsbi-openwrt-qemu.XXXXXX")
  QEMU_INPUT="${QEMU_INPUT_DIR}/console"
  mkfifo "$QEMU_INPUT"
  exec 3<>"$QEMU_INPUT"
}

start_qemu_sbi() {
  create_qemu_console
  qemu-system-riscv64 \
    -machine virt \
    -nographic \
    -no-reboot \
    -smp 1 \
    -m 1G \
    -bios "$RUSTSBI_ELF" \
    -kernel "$KERNEL_IMAGE" \
    -initrd "$INITRAMFS" \
    -append 'console=ttyS0 earlycon=sbi' \
    <"$QEMU_INPUT" >"$LOG_FILE" 2>&1 &
  QEMU_PID=$!
}

start_qemu_u_boot() {
  create_qemu_console
  qemu-system-riscv64 \
    -machine virt \
    -nographic \
    -no-reboot \
    -smp 1 \
    -m 1G \
    -bios "$UB_SPL" \
    -device "loader,file=${UB_ITB},addr=0x80200000" \
    -device "loader,file=${KERNEL_IMAGE},addr=${KERNEL_LOAD_ADDRESS}" \
    -device "loader,file=${INITRAMFS},addr=${INITRAMFS_LOAD_ADDRESS}" \
    <"$QEMU_INPUT" >"$LOG_FILE" 2>&1 &
  QEMU_PID=$!
}

userspace_is_ready() {
  grep -Fq 'SBI implementation ID=0x4' "$LOG_FILE" \
    && grep -Fq "$SMOKE_MARKER" "$LOG_FILE"
}

boot_has_failed() {
  grep -Eq "$BOOT_FAILURE_PATTERN" "$LOG_FILE"
}

report_early_exit() {
  local qemu_exit
  set +e
  wait "$QEMU_PID"
  qemu_exit=$?
  set -e
  echo "QEMU exited before OpenWrt reached userspace (exit=${qemu_exit})" >&2
  tail -n 160 "$LOG_FILE" || true
}

wait_for_userspace() {
  local tick
  local autoboot_interrupted=false
  local boot_command_sent=false
  local console_activated=false
  local smoke_command_sent=false

  if [[ "$BOOT_MODE" = sbi ]]; then
    autoboot_interrupted=true
    boot_command_sent=true
  fi

  for ((tick = 0; tick < BOOT_TIMEOUT_SECS * 10; tick++)); do
    if userspace_is_ready; then
      return 0
    fi
    if boot_has_failed; then
      echo "OpenWrt reported a fatal boot error" >&2
      grep -E --max-count=5 "$BOOT_FAILURE_PATTERN" "$LOG_FILE" >&2 || true
      tail -n 160 "$LOG_FILE" || true
      return 1
    fi
    if ! kill -0 "$QEMU_PID" 2>/dev/null; then
      userspace_is_ready && return 0
      report_early_exit
      return 1
    fi

    if [[ "$autoboot_interrupted" = false ]] \
      && grep -Fq 'Hit any key to stop autoboot' "$LOG_FILE"; then
      printf '\n' >&3
      autoboot_interrupted=true
    fi
    if [[ "$boot_command_sent" = false ]] && grep -Fq '=>' "$LOG_FILE"; then
      # shellcheck disable=SC2016 # U-Boot expands fdtcontroladdr in the guest.
      printf 'setenv bootargs console=ttyS0 earlycon=sbi; booti %s %s:%s ${fdtcontroladdr}\n' \
        "$KERNEL_LOAD_ADDRESS" "$INITRAMFS_LOAD_ADDRESS" "$INITRAMFS_SIZE_HEX" >&3
      boot_command_sent=true
      echo "Sent OpenWrt boot command to U-Boot"
    fi
    if [[ "$console_activated" = false ]] \
      && grep -Fq 'Please press Enter to activate this console' "$LOG_FILE"; then
      printf '\n' >&3
      console_activated=true
    fi
    if [[ "$smoke_command_sent" = false ]] \
      && grep -Eq 'root@[^[:space:]]*:[^#]*#' "$LOG_FILE"; then
      printf '%s\n' "$SMOKE_COMMAND" >&3
      smoke_command_sent=true
      echo "Sent OpenWrt smoke command over the serial console"
    fi
    sleep 0.1
  done

  userspace_is_ready && return 0
  echo "OpenWrt did not pass its smoke test within ${BOOT_TIMEOUT_SECS}s" >&2
  tail -n 160 "$LOG_FILE" || true
  return 1
}

main() {
  trap cleanup EXIT

  check_prerequisites
  download_openwrt_image
  prepare_openwrt_assets
  if [[ "$BOOT_MODE" = u-boot ]]; then
    prepare_u_boot_source
    build_u_boot
    start_qemu_u_boot
  else
    start_qemu_sbi
  fi
  wait_for_userspace

  echo "RustSBI booted OpenWrt ${OPENWRT_RELEASE} to userspace successfully (${BOOT_MODE})"
  echo "QEMU log: ${LOG_FILE}"
}

main "$@"
