#!/usr/bin/env bash
set -euo pipefail

readonly FREEBSD_RELEASE="14.1"
readonly FREEBSD_IMAGE="FreeBSD-${FREEBSD_RELEASE}-RELEASE-riscv-riscv64.raw"
readonly FREEBSD_ARCHIVE="${FREEBSD_IMAGE}.xz"
readonly FREEBSD_BASE_URL="https://archive.freebsd.org/old-releases/VM-IMAGES/${FREEBSD_RELEASE}-RELEASE/riscv64/Latest"
readonly FREEBSD_SHA256="509b24437156e65e031ca12219a408460743f0f0e82368ce4afa83dddbb4806e"
readonly FREEBSD_KERNEL_BASE_URL="https://archive.freebsd.org/old-releases/riscv/riscv64/${FREEBSD_RELEASE}-RELEASE"
readonly FREEBSD_KERNEL_ARCHIVE="kernel.txz"
readonly FREEBSD_KERNEL_SHA256="321e538f9078659d8852e9a1628febac013fc3167e22e782266039a4b54fd3b7"
readonly UBOOT_REPOSITORY="https://github.com/u-boot/u-boot.git"
readonly UBOOT_VERSION="v2024.04"
readonly UBOOT_COMMIT="25049ad560826f7dc1c4740883b0016014a59789"

if (( $# > 1 )); then
  echo "Usage: $0 [sbi|u-boot]" >&2
  exit 2
fi
readonly BOOT_MODE="${1:-u-boot}"
case "$BOOT_MODE" in
  sbi | u-boot) ;;
  *)
    echo "Unknown FreeBSD boot mode: ${BOOT_MODE}" >&2
    echo "Usage: $0 [sbi|u-boot]" >&2
    exit 2
    ;;
esac

readonly FREEBSD_CACHE_DIR="${FREEBSD_CACHE_DIR:-.cache/freebsd/${FREEBSD_RELEASE}}"
readonly FREEBSD_ARCHIVE_PATH="${FREEBSD_CACHE_DIR}/${FREEBSD_ARCHIVE}"
readonly FREEBSD_IMAGE_PATH="${FREEBSD_CACHE_DIR}/${FREEBSD_IMAGE}"
readonly FREEBSD_KERNEL_ARCHIVE_PATH="${FREEBSD_CACHE_DIR}/${FREEBSD_KERNEL_ARCHIVE}"
readonly FREEBSD_KERNEL_PATH="${FREEBSD_CACHE_DIR}/kernel"
readonly UBOOT_CACHE_DIR="${UBOOT_CACHE_DIR:-.cache/u-boot/${UBOOT_VERSION}}"
readonly UBOOT_BUILD_DIR="${UBOOT_BUILD_DIR:-target/u-boot-freebsd}"
readonly RUSTSBI="${FREEBSD_RUSTSBI:-target/riscv64gc-unknown-none-elf/release/rustsbi-prototyper-dynamic.bin}"
readonly LOG_DIR="${QEMU_LOG_DIR:-qemu-logs}"
readonly LOG_FILE="${LOG_DIR}/prototyper-freebsd-${BOOT_MODE}.log"
readonly BOOT_TIMEOUT_SECS="${FREEBSD_BOOT_TIMEOUT_SECS:-600}"
readonly DOWNLOAD_CONNECT_TIMEOUT_SECS="${FREEBSD_DOWNLOAD_CONNECT_TIMEOUT_SECS:-30}"
readonly DOWNLOAD_TIMEOUT_SECS="${FREEBSD_DOWNLOAD_TIMEOUT_SECS:-1800}"

QEMU_PID=""
DOWNLOAD_TEMP=""
IMAGE_TEMP=""
KERNEL_TEMP=""

verify_sha256() {
  local digest=$1 path=$2
  [[ -s "$path" ]] || return 1
  printf '%s  %s\n' "$digest" "$path" | sha256sum --check --status
}

verify_freebsd_archive() {
  verify_sha256 "$FREEBSD_SHA256" "$FREEBSD_ARCHIVE_PATH"
}

download_freebsd_image() {
  local archive_refreshed=0
  mkdir -p "$FREEBSD_CACHE_DIR"

  if ! verify_freebsd_archive; then
    rm -f "$FREEBSD_ARCHIVE_PATH"
    echo "Downloading FreeBSD ${FREEBSD_RELEASE} VM image"
    DOWNLOAD_TEMP=$(mktemp "${FREEBSD_ARCHIVE_PATH}.part.XXXXXX")
    if ! curl --fail --location \
      --connect-timeout "$DOWNLOAD_CONNECT_TIMEOUT_SECS" \
      --max-time "$DOWNLOAD_TIMEOUT_SECS" \
      --retry 3 \
      --retry-all-errors \
      --retry-max-time "$DOWNLOAD_TIMEOUT_SECS" \
      --output "$DOWNLOAD_TEMP" \
      "${FREEBSD_BASE_URL}/${FREEBSD_ARCHIVE}"; then
      rm -f "$DOWNLOAD_TEMP"
      DOWNLOAD_TEMP=""
      return 1
    fi
    if ! verify_sha256 "$FREEBSD_SHA256" "$DOWNLOAD_TEMP"; then
      echo "Checksum mismatch for ${FREEBSD_ARCHIVE}" >&2
      rm -f "$DOWNLOAD_TEMP"
      DOWNLOAD_TEMP=""
      return 1
    fi
    mv "$DOWNLOAD_TEMP" "$FREEBSD_ARCHIVE_PATH"
    DOWNLOAD_TEMP=""
    archive_refreshed=1
  fi
  verify_freebsd_archive

  if (( archive_refreshed )) || [[ ! -s "$FREEBSD_IMAGE_PATH" ]]; then
    echo "Extracting ${FREEBSD_ARCHIVE}"
    rm -f "$FREEBSD_IMAGE_PATH"
    IMAGE_TEMP=$(mktemp "${FREEBSD_IMAGE_PATH}.part.XXXXXX")
    if ! xz --decompress --stdout "$FREEBSD_ARCHIVE_PATH" >"$IMAGE_TEMP"; then
      rm -f "$IMAGE_TEMP"
      IMAGE_TEMP=""
      return 1
    fi
    mv "$IMAGE_TEMP" "$FREEBSD_IMAGE_PATH"
    IMAGE_TEMP=""
  fi
  test -s "$FREEBSD_IMAGE_PATH"
}

verify_freebsd_kernel_archive() {
  verify_sha256 "$FREEBSD_KERNEL_SHA256" "$FREEBSD_KERNEL_ARCHIVE_PATH"
}

download_freebsd_kernel() {
  mkdir -p "$FREEBSD_CACHE_DIR"

  if ! verify_freebsd_kernel_archive; then
    rm -f "$FREEBSD_KERNEL_ARCHIVE_PATH"
    echo "Downloading FreeBSD ${FREEBSD_RELEASE} kernel"
    DOWNLOAD_TEMP=$(mktemp "${FREEBSD_KERNEL_ARCHIVE_PATH}.part.XXXXXX")
    if ! curl --fail --location \
      --connect-timeout "$DOWNLOAD_CONNECT_TIMEOUT_SECS" \
      --max-time "$DOWNLOAD_TIMEOUT_SECS" \
      --retry 3 \
      --retry-all-errors \
      --retry-max-time "$DOWNLOAD_TIMEOUT_SECS" \
      --output "$DOWNLOAD_TEMP" \
      "${FREEBSD_KERNEL_BASE_URL}/${FREEBSD_KERNEL_ARCHIVE}"; then
      rm -f "$DOWNLOAD_TEMP"
      DOWNLOAD_TEMP=""
      return 1
    fi
    if ! verify_sha256 "$FREEBSD_KERNEL_SHA256" "$DOWNLOAD_TEMP"; then
      echo "Checksum mismatch for ${FREEBSD_KERNEL_ARCHIVE}" >&2
      rm -f "$DOWNLOAD_TEMP"
      DOWNLOAD_TEMP=""
      return 1
    fi
    mv "$DOWNLOAD_TEMP" "$FREEBSD_KERNEL_ARCHIVE_PATH"
    DOWNLOAD_TEMP=""
  fi
  verify_freebsd_kernel_archive

  echo "Extracting FreeBSD kernel"
  KERNEL_TEMP=$(mktemp "${FREEBSD_KERNEL_PATH}.part.XXXXXX")
  if ! tar --extract --xz --to-stdout \
    --file "$FREEBSD_KERNEL_ARCHIVE_PATH" \
    ./boot/kernel/kernel >"$KERNEL_TEMP"; then
    rm -f "$KERNEL_TEMP"
    KERNEL_TEMP=""
    return 1
  fi
  mv "$KERNEL_TEMP" "$FREEBSD_KERNEL_PATH"
  KERNEL_TEMP=""
  test -s "$FREEBSD_KERNEL_PATH"
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
  local rustsbi_path build_path
  rustsbi_path=$(realpath "$RUSTSBI")
  build_path=$(realpath -m "$UBOOT_BUILD_DIR")
  test -s "$rustsbi_path"
  mkdir -p "$build_path"

  make -C "$UBOOT_CACHE_DIR" O="$build_path" \
    ARCH=riscv CROSS_COMPILE=riscv64-linux-gnu- qemu-riscv64_spl_defconfig
  make -C "$UBOOT_CACHE_DIR" O="$build_path" \
    ARCH=riscv CROSS_COMPILE=riscv64-linux-gnu- OPENSBI="$rustsbi_path" \
    -j"$(nproc)"
  test -s "${build_path}/spl/u-boot-spl"
  test -s "${build_path}/u-boot.itb"
}

stop_qemu() {
  if [[ -n "$QEMU_PID" ]] && kill -0 "$QEMU_PID" 2>/dev/null; then
    kill "$QEMU_PID" 2>/dev/null || true
    wait "$QEMU_PID" 2>/dev/null || true
  fi
}

cleanup() {
  if [[ -n "$DOWNLOAD_TEMP" ]]; then
    rm -f "$DOWNLOAD_TEMP"
  fi
  if [[ -n "$IMAGE_TEMP" ]]; then
    rm -f "$IMAGE_TEMP"
  fi
  if [[ -n "$KERNEL_TEMP" ]]; then
    rm -f "$KERNEL_TEMP"
  fi
  stop_qemu
}

start_qemu_process() {
  mkdir -p "$LOG_DIR"
  : >"$LOG_FILE"

  qemu-system-riscv64 \
    -machine virt \
    -nographic \
    -no-reboot \
    -snapshot \
    -smp 1 \
    -m 1G \
    "$@" \
    </dev/null >"$LOG_FILE" 2>&1 &
  QEMU_PID=$!
}

start_qemu() {
  local build_path image_path
  build_path=$(realpath "$UBOOT_BUILD_DIR")
  image_path=$(realpath "$FREEBSD_IMAGE_PATH")
  validate_qemu_path "$build_path" "U-Boot build path"
  validate_qemu_path "$image_path" "FreeBSD image path"

  start_qemu_process \
    -bios "${build_path}/spl/u-boot-spl" \
    -device "loader,file=${build_path}/u-boot.itb,addr=0x80200000" \
    -drive "file=${image_path},format=raw,if=none,id=hd0" \
    -device virtio-blk-device,drive=hd0
}

start_qemu_sbi() {
  local image_path kernel_path
  image_path=$(realpath "$FREEBSD_IMAGE_PATH")
  kernel_path=$(realpath "$FREEBSD_KERNEL_PATH")
  validate_qemu_path "$image_path" "FreeBSD image path"

  start_qemu_process \
    -bios "$RUSTSBI" \
    -kernel "$kernel_path" \
    -append "vfs.root.mountfrom=ufs:/dev/gpt/rootfs" \
    -drive "file=${image_path},format=raw,if=none,id=hd0" \
    -device virtio-blk-device,drive=hd0
}

qemu_is_running() {
  kill -0 "$QEMU_PID" 2>/dev/null
}

validate_qemu_path() {
  local path=$1 description=$2
  case "$path" in
    *,*)
      echo "QEMU path cannot contain a comma (${description}): ${path}" >&2
      return 1
      ;;
  esac
}

boot_failed() {
  grep -Eiq \
    'panic:|fatal trap|kernel page fault|no bootable device|failed to boot|mounting from .* failed|cannot mount root|can.t find root' \
    "$LOG_FILE"
}

root_is_mounted() {
  grep -Eiq 'trying to mount root from|mounting root from' "$LOG_FILE"
}

login_is_ready() {
  grep -Fq 'login:' "$LOG_FILE"
}

print_log_path() {
  echo "QEMU log: ${LOG_FILE}"
}

report_log_path() {
  print_log_path >&2
}

report_log_tail() {
  tail -n 160 "$LOG_FILE" >&2 || true
}

report_boot_failure() {
  echo "FreeBSD reported a fatal boot error:" >&2
  report_log_path
  grep -Eim 5 \
    'panic:|fatal trap|kernel page fault|no bootable device|failed to boot|mounting from .* failed|cannot mount root|can.t find root' \
    "$LOG_FILE" >&2 || true
  report_log_tail
}

report_early_exit() {
  local qemu_exit
  set +e
  wait "$QEMU_PID"
  qemu_exit=$?
  set -e

  echo "QEMU exited before FreeBSD reached a login prompt (exit=${qemu_exit})" >&2
  report_log_path
  report_log_tail
}

wait_for_freebsd() {
  local elapsed
  for ((elapsed = 0; elapsed < BOOT_TIMEOUT_SECS; elapsed++)); do
    # Check the success markers first: FreeBSD may power off or emit a late
    # diagnostic immediately after the login prompt appears.
    if root_is_mounted && login_is_ready; then
      return 0
    fi
    if boot_failed; then
      report_boot_failure
      return 1
    fi
    if ! qemu_is_running; then
      report_early_exit
      return 1
    fi
    sleep 1
  done

  if root_is_mounted && login_is_ready; then
    return 0
  fi

  echo "FreeBSD did not reach a login prompt within ${BOOT_TIMEOUT_SECS}s" >&2
  report_log_path
  echo "For a slow local host, retry with FREEBSD_BOOT_TIMEOUT_SECS=<seconds>." >&2
  report_log_tail
  return 1
}

main() {
  trap cleanup EXIT
  echo "FreeBSD boot timeout: ${BOOT_TIMEOUT_SECS}s (override with FREEBSD_BOOT_TIMEOUT_SECS=<seconds>)" >&2
  download_freebsd_image
  test -s "$RUSTSBI"
  qemu-system-riscv64 --version
  if [[ "$BOOT_MODE" = u-boot ]]; then
    prepare_u_boot_source
    build_u_boot
    start_qemu
  else
    download_freebsd_kernel
    start_qemu_sbi
  fi
  wait_for_freebsd

  echo "RustSBI booted FreeBSD ${FREEBSD_RELEASE} to a login prompt successfully (${BOOT_MODE})"
  print_log_path
}

main "$@"
