#!/usr/bin/env bash
#
# Boot the documented PolyOS mobile image through RustSBI Prototyper and
# U-Boot in QEMU, following
# firmware/docs/booting-polyos-in-qemu-using-uboot-and-rustsbi.md.
#
# The chain is QEMU -> u-boot-spl -> RustSBI -> u-boot.itb -> Linux -> PolyOS
# userspace. The boot partition carries the vendor kernel and initramfs plus
# an extlinux configuration; U-Boot picks it off the AHCI disk while the six
# OpenHarmony partition images stay on virtio-mmio, so the fixed kernel
# command line still sees them as /dev/block/vda..vdf.
#
# The distribution image is fetched from a versioned, checksum-pinned URL and
# rebuilt into the boot disk on every run; only the extracted images are
# cached. RustSBI is rebuilt from the commit under test and is never cached,
# and neither is the U-Boot build: the SPL embeds the firmware binary.
#
# Requires: `cargo prototyper build` to have produced the firmware, plus
# qemu-system-riscv64, qemu-img, curl, xz, parted, debugfs (e2fsprogs),
# riscv64-linux-gnu-gcc, swig, python3-dev and libssl-dev for the U-Boot
# build. Everything runs unprivileged: the boot disk is assembled with
# parted/dd/debugfs on plain files, with no loop devices or mounts.

set -euo pipefail

if (( $# > 0 )); then
  echo "Usage: $0" >&2
  exit 2
fi

readonly POLYOS_VERSION="3.2-release"
readonly POLYOS_URL="https://polyos.iscas.ac.cn/downloads/polyos-mobile-${POLYOS_VERSION}.img.tar.xz"
readonly POLYOS_SHA256="e6a69fc0c55096062c3362d0abf2705ae7d1972064bfe7e5d340266bf69d4599"

readonly UB_VERSION="2024.04"
readonly UB_URL="https://github.com/u-boot/u-boot/archive/refs/tags/v${UB_VERSION}.tar.gz"
readonly UB_SHA256="d6b57ce574a0a0504a5b6596644ceacb7f77bde9353779bcf2fde07c4b9a2b92"

readonly CROSS_COMPILE="riscv64-linux-gnu-"

# PolyOS reports readiness through the parameter service: this marker is set
# once the application framework is up, i.e. the image reached userspace.
readonly READY_MARKER="bootevent.appfwk.ready"
# Text that means the boot already went wrong. The U-Boot trap entry covers
# the crash loop this setup produces when the device tree handoff goes bad;
# without it the job would sit out the whole timeout.
readonly BOOT_FAILURE_PATTERN="Kernel panic|not syncing|Attempted to kill init|Unhandled exception"

readonly CACHE_DIR="${POLYOS_CACHE_DIR:-.cache/polyos}"
readonly WORK_DIR="${POLYOS_WORK_DIR:-.polyos/work}"
readonly LOG_DIR="${QEMU_LOG_DIR:-qemu-logs}"
readonly LOG_FILE="${LOG_DIR}/prototyper-polyos-uboot.log"

# The U-Boot path embeds the flattened firmware binary in the SPL's FIT image.
readonly RUSTSBI="${POLYOS_RUSTSBI_BIN:-target/riscv64gc-unknown-none-elf/release/rustsbi-prototyper.bin}"
readonly BOOT_TIMEOUT_SECS="${POLYOS_BOOT_TIMEOUT_SECS:-300}"

readonly POLYOS_ARCHIVE="${CACHE_DIR}/polyos-mobile-${POLYOS_VERSION}.img.tar.xz"
readonly IMAGE_DIR="${WORK_DIR}/images"
readonly BOOT_EXT4="${WORK_DIR}/boot.ext4"
readonly BOOT_IMAGE="${WORK_DIR}/boot.img"
readonly EXTLINUX_CONF="${WORK_DIR}/extlinux.conf"

UB_SPL=""
UB_ITB=""

readonly DOWNLOAD_CONNECT_TIMEOUT_SECS="${POLYOS_DOWNLOAD_CONNECT_TIMEOUT_SECS:-30}"
readonly DOWNLOAD_TIMEOUT_SECS="${POLYOS_DOWNLOAD_TIMEOUT_SECS:-1800}"

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

# Unpack the vendor images. The archive holds an images/ directory with the
# boot filesystem plus the six partition images the kernel command line mounts
# from virtio disks; the bundled kernel and initramfs stay inside boot.ext4.
prepare_polyos() {
  local image
  local -a members=(
    images/boot.ext4
    images/updater.img
    images/system.img
    images/vendor.img
    images/userdata.img
    images/sys_prod.img
    images/chip_prod.img
  )

  mkdir -p "$CACHE_DIR" "$WORK_DIR"
  download_asset "$POLYOS_URL" "$POLYOS_ARCHIVE" "$POLYOS_SHA256"

  # Reuse an earlier extraction only when it provably came from the pinned
  # archive: a stamp file records the archive digest, so bumping the pinned
  # version or checksum without bumping the workflow cache key cannot make a
  # stale image slip through.
  local stamp="${IMAGE_DIR}/.archive-sha256"
  local missing=0
  for image in "${members[@]}"; do
    [[ -s "${IMAGE_DIR}/${image#images/}" ]] || missing=1
  done
  if (( missing == 0 )) && [[ -f "$stamp" ]] \
    && [[ "$(<"$stamp")" == "$POLYOS_SHA256" ]]; then
    echo "Using extracted PolyOS ${POLYOS_VERSION} images" >&2
    return
  fi

  rm -rf "$IMAGE_DIR"
  mkdir -p "$IMAGE_DIR"
  tar -xJf "$POLYOS_ARCHIVE" -C "$WORK_DIR" "${members[@]}"
  for image in "${members[@]}"; do
    test -s "${IMAGE_DIR}/${image#images/}"
  done
  echo "$POLYOS_SHA256" >"$stamp"
}

# Assemble the GPT boot disk the guide describes: partition 1 holds the vendor
# boot filesystem, extended with the extlinux configuration that U-Boot's
# distro boot reads. debugfs writes into the ext4 image directly, so no loop
# device or mount is needed. The `loglevel=1` the guide ships is raised to 7:
# at level 1 the kernel hides the userspace readiness markers this test waits
# for, while panics still print at any level.
build_boot_image() {
  cp "${IMAGE_DIR}/boot.ext4" "$BOOT_EXT4"

  cat >"$EXTLINUX_CONF" <<'EOF'
default polyOS-RISC-V
label   polyOS-RISC-V
    kernel /Image
    initrd /ramdisk.img
    append 'loglevel=7 ip=192.168.137.2:192.168.137.1:192.168.137.1:255.255.255.0::eth0:off sn=0023456789 console=tty0,115200 console=ttyS0,115200 init=/bin/init ohos.boot.hardware=virt root=/dev/ram0 rw ohos.required_mount.system=/dev/block/vdb@/usr@ext4@ro,barrier=1@wait,required ohos.required_mount.vendor=/dev/block/vdc@/vendor@ext4@ro,barrier=1@wait,required ohos.required_mount.sys_prod=/dev/block/vde@/sys_prod@ext4@ro,barrier=1@wait,required ohos.required_mount.chip_prod=/dev/block/vdf@/chip_prod@ext4@ro,barrier=1@wait,required ohos.required_mount.data=/dev/block/vdd@/data@ext4@nosuid,nodev,noatime,barrier=1,data=ordered,noauto_da_alloc@wait,reservedsize=1073741824 ohos.required_mount.misc=/dev/block/vda@/misc@none@none=@wait,required'
EOF

  debugfs -w -R "mkdir /extlinux" "$BOOT_EXT4" >/dev/null 2>&1 || true
  debugfs -w -R "rm /extlinux/extlinux.conf" "$BOOT_EXT4" >/dev/null 2>&1 || true
  debugfs -w -R "write ${EXTLINUX_CONF} /extlinux/extlinux.conf" "$BOOT_EXT4"
  debugfs -R "cat /extlinux/extlinux.conf" "$BOOT_EXT4" >/dev/null

  rm -f "$BOOT_IMAGE"
  qemu-img create -q -f raw "$BOOT_IMAGE" 1g
  parted -s "$BOOT_IMAGE" mklabel gpt
  parted -s "$BOOT_IMAGE" mkpart primary ext4 2048s 100%
  dd if="$BOOT_EXT4" of="$BOOT_IMAGE" bs=512 seek=2048 conv=notrunc status=none
}

# Build U-Boot with the RustSBI firmware embedded as the OpenSBI payload.
#
# The stock boot flow faults in bootm once it starts using the control tree
# where U-Boot's relocation puts it: just below U-Boot itself at the top of
# RAM, growing the tree for the long PolyOS bootargs runs into the firmware's
# own area. Capping fdt_high at 0x88000000 makes bootm move the working copy
# into the low reserved area instead, the same lever the Arch (#351) and
# zCore boot jobs use for the same failure family.
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

  "${tree}/scripts/config" --file "${tree}/.config" --enable USE_BOOTCOMMAND
  "${tree}/scripts/config" --file "${tree}/.config" --set-str BOOTCOMMAND \
    'setenv fdt_high 0x88000000; run distro_bootcmd'

  make -C "$tree" ARCH=riscv CROSS_COMPILE="$CROSS_COMPILE" \
    OPENSBI="$rustsbi_abs" -j"$(nproc)"

  UB_SPL="${tree}/spl/u-boot-spl"
  UB_ITB="${tree}/u-boot.itb"
  test -s "$UB_SPL" && test -s "$UB_ITB"

  # The SPL FIT must carry the firmware built from the commit under test, not
  # a stale object: dump the embedded image and compare it byte for byte.
  "${tree}/tools/dumpimage" -T flat_dt -p 1 -o "${WORK_DIR}/rustsbi-from-fit.bin" "$UB_ITB" \
    >"${WORK_DIR}/fit-layout.log"
  cmp "$rustsbi_abs" "${WORK_DIR}/rustsbi-from-fit.bin"
  echo "Verified u-boot.itb contains current-commit RustSBI" >&2
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
  # Truncate any stale log from a previous run before QEMU starts, so the
  # wait loop cannot mistake an old success marker for this boot's. Record
  # the toolchain version in the log itself so an uploaded artifact carries
  # its own provenance.
  : >"$LOG_FILE"
  echo "QEMU: $(qemu-system-riscv64 --version | head -n 1)" >>"$LOG_FILE"
  # The documented topology: the boot disk sits on AHCI/SATA (the U-Boot
  # distro boot scans it as scsi 0:1) while the six system images stay on
  # virtio-mmio in the documented order, so the fixed kernel command line
  # still sees them as /dev/block/vda..vdf. Headless: the guide's
  # `-display sdl,gl=off` becomes `-nographic` for CI.
  #
  # Every drive is attached with `snapshot=on`: the guest mounts /data
  # read-write off userdata.img, and without a discardable overlay the
  # writes would land in the cached extracted images, so the pinned input
  # would silently drift between the first and later runs.
  qemu-system-riscv64 \
    -name PolyOS-Mobile \
    -machine virt \
    -m 4096 \
    -smp 4 \
    -nographic \
    -no-reboot \
    -bios "$UB_SPL" \
    -device "loader,file=${UB_ITB},addr=0x80200000" \
    -drive "if=none,file=${BOOT_IMAGE},format=raw,id=boot,snapshot=on" \
    -device ahci,id=ahci -device "ide-hd,bus=ahci.0,drive=boot" \
    -drive "if=none,file=${IMAGE_DIR}/updater.img,format=raw,id=updater,snapshot=on" \
    -device virtio-blk-device,drive=updater \
    -drive "if=none,file=${IMAGE_DIR}/system.img,format=raw,id=system,snapshot=on" \
    -device virtio-blk-device,drive=system \
    -drive "if=none,file=${IMAGE_DIR}/vendor.img,format=raw,id=vendor,snapshot=on" \
    -device virtio-blk-device,drive=vendor \
    -drive "if=none,file=${IMAGE_DIR}/userdata.img,format=raw,id=userdata,snapshot=on" \
    -device virtio-blk-device,drive=userdata \
    -drive "if=none,file=${IMAGE_DIR}/sys_prod.img,format=raw,id=sys-prod,snapshot=on" \
    -device virtio-blk-device,drive=sys-prod \
    -drive "if=none,file=${IMAGE_DIR}/chip_prod.img,format=raw,id=chip-prod,snapshot=on" \
    -device virtio-blk-device,drive=chip-prod \
    </dev/null >>"$LOG_FILE" 2>&1 &
  QEMU_PID=$!
}

# The marker alone does not prove this chain ran: require the RustSBI banner
# and the U-Boot handoff into Linux as well, so a swapped-in firmware or a
# boot that never left U-Boot cannot pass.
userspace_is_ready() {
  grep -Fq '[RustSBI]' "$LOG_FILE" \
    && grep -Fq 'Starting kernel ...' "$LOG_FILE" \
    && grep -Fq "$READY_MARKER" "$LOG_FILE"
}

boot_has_failed() {
  grep -Eq "$BOOT_FAILURE_PATTERN" "$LOG_FILE"
}

report_boot_failure() {
  echo "PolyOS failed to boot:" >&2
  grep -E --max-count=5 "$BOOT_FAILURE_PATTERN" "$LOG_FILE" >&2 || true
  tail -n 120 "$LOG_FILE" || true
}

report_early_exit() {
  local qemu_exit
  set +e
  wait "$QEMU_PID"
  qemu_exit=$?
  set -e

  echo "QEMU exited before PolyOS reached userspace (exit=${qemu_exit})" >&2
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

  echo "PolyOS did not reach userspace within ${BOOT_TIMEOUT_SECS}s" >&2
  tail -n 120 "$LOG_FILE" || true
  return 1
}

main() {
  trap cleanup EXIT

  check_prerequisites
  prepare_polyos
  build_boot_image
  build_uboot
  start_qemu
  wait_for_userspace

  echo "RustSBI booted PolyOS ${POLYOS_VERSION} to userspace successfully (u-boot)"
  echo "QEMU log: ${LOG_FILE}"
}

main "$@"
