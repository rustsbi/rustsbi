#!/usr/bin/env bash
#
# Boot the eweOS riscv64 live ISO through RustSBI Prototyper in QEMU.
#
# Usage: $0 [sbi|u-boot|edk2|check-iso|resolve-iso]
#
# `check-iso` only verifies an already cached ISO against the pinned digest,
# so CI can reject a stale cache entry right after restoring it. A missing
# ISO passes; the boot step downloads and verifies it.
#
# Boot paths, all ending in the ISO's own kernel, initramfs and live root:
#
#   sbi     QEMU -> RustSBI (dynamic) -> Linux -> eweOS userspace.
#   u-boot  QEMU -> RustSBI (dynamic) -> U-Boot (S-mode) -> Linux (booti).
#           U-Boot cannot read ISO9660, so the kernel and initramfs are copied
#           onto a small FAT disk that U-Boot loads them from.
#   edk2    QEMU -> RustSBI (dynamic) -> EDK II -> the ISO's Limine EFI
#           loader -> Linux. A FAT boot disk contains the unchanged EFI
#           loader, kernel and initramfs extracted from the verified ISO.
#           A CI-specific Limine config enables serial output instead of the
#           ISO's quiet splash defaults; the original ISO supplies the live root.
#
# eweOS publishes a rolling ISO. Resolve its official checksum once per CI
# boot job and use it for that job's cache key and image checks. Every restored or
# downloaded image is checked against that digest. Set EWEOS_ISO_SHA256 (and
# optionally EWEOS_ISO_URL) to reproduce a particular image while available.
# If upstream replaces the ISO during a run, fail rather than accept an
# unverified image or silently change the digest midway through the run.
#
# The success condition is dinit, running from the live root filesystem,
# bringing up rc.target and the greetd login manager with no service failing
# on the way: that means RustSBI (and the bootloader, if any) handed over to
# Linux, the initramfs found the ISO and switched root, and the distribution's
# own init reached its normal boot target.
#
# QEMU is built from a pinned release rather than taken from the distribution.
# Under Ubuntu 24.04's QEMU 8.2.2 this boot stalls at random: one vCPU sits at
# full load while a kernel memcpy crawls along at a few hundred bytes per
# second, so a dinit service overruns its 60 second start timeout. The stall
# happens with QEMU's bundled OpenSBI as well as with RustSBI, and with Sstc
# disabled; it has not been seen with QEMU 10.2.2. EWEOS_QEMU can point at
# another qemu-system-riscv64 for local runs.
#
# Requires: `cargo prototyper build` to have produced the firmware, plus curl,
# bsdtar and the QEMU build dependencies (a C toolchain, ninja, pkg-config,
# python3-venv, glib and pixman). The u-boot path also needs a riscv64
# cross compiler, the U-Boot host build dependencies, dosfstools and mtools;
# the edk2 path needs git, acpica-tools, nasm, uuid-dev, dosfstools and mtools.

set -euo pipefail

if (( $# > 1 )); then
  echo "Usage: $0 [sbi|u-boot|edk2|check-iso|resolve-iso]" >&2
  exit 2
fi

readonly BOOT_MODE="${1:-sbi}"
case "$BOOT_MODE" in
  sbi | u-boot | edk2 | check-iso | resolve-iso) ;;
  *)
    echo "Usage: $0 [sbi|u-boot|edk2|check-iso|resolve-iso]" >&2
    exit 2
    ;;
esac

readonly ISO_NAME="eweos-riscv64-liveimage-standard.iso"
readonly ISO_URL="${EWEOS_ISO_URL:-https://os-repo.ewe.moe/eweos-images/${ISO_NAME}}"
# Resolve through the publisher's HTTPS checksum endpoint, not from the ISO
# we downloaded. This checks integrity, not independent release signing.
resolve_iso_digest() {
  local manifest digest filename extra
  manifest=$(curl --fail --silent --show-error --location \
    --connect-timeout 30 --max-time 60 --retry 3 \
    "${ISO_URL}.sha256") || return 1
  if [[ "$manifest" == *$'\n'* ]]; then
    echo "Expected a single ISO checksum entry" >&2
    return 1
  fi
  read -r digest filename extra <<<"$manifest"
  if [[ ! "$digest" =~ ^[0-9a-f]{64}$ || "$filename" != "$ISO_NAME" || -n "$extra" ]]; then
    echo "Invalid eweOS checksum manifest" >&2
    return 1
  fi
  printf '%s\n' "$digest"
}

if [[ "$BOOT_MODE" = resolve-iso ]]; then
  resolve_iso_digest
  exit
fi
ISO_SHA256="${EWEOS_ISO_SHA256:-}"
if [[ -z "$ISO_SHA256" ]]; then
  ISO_SHA256=$(resolve_iso_digest)
fi
if [[ ! "$ISO_SHA256" =~ ^[0-9a-f]{64}$ ]]; then
  echo "Invalid EWEOS_ISO_SHA256" >&2
  exit 2
fi
readonly ISO_SHA256

readonly UB_VERSION="2024.04"
readonly UB_URL="https://github.com/u-boot/u-boot/archive/refs/tags/v${UB_VERSION}.tar.gz"
readonly UB_SHA256="d6b57ce574a0a0504a5b6596644ceacb7f77bde9353779bcf2fde07c4b9a2b92"

readonly EDK2_VERSION="edk2-stable202505"
readonly EDK2_COMMIT="6951dfe7d59d144a3a980bd7eda699db2d8554ac"
readonly EDK2_URL="https://github.com/tianocore/edk2.git"

readonly QEMU_VERSION="10.2.2"
readonly QEMU_URL="https://download.qemu.org/qemu-${QEMU_VERSION}.tar.xz"
readonly QEMU_SHA256="784b296ff29c1417aa72323abcb2d2ea9ab9771724f577dcd785c3b04f21e176"

readonly CROSS_COMPILE="riscv64-linux-gnu-"

# `ram=0` keeps the live squashfs images on the medium instead of copying
# them into a tmpfs first, which saves a gigabyte of guest memory and the copy
# time. Every path passes the same command line.
readonly KERNEL_CMDLINE="console=ttyS0 ram=0"

# The live system prints dinit's service status on the serial console. greetd
# is the last service the live profile enables, so seeing it start alongside
# rc.target means the boot finished rather than merely got under way.
SUCCESS_MARKERS=("[  OK  ] rc.target" "[  OK  ] greetd")

# Each bootloader path must also prove that the kernel really came through
# that bootloader, not merely reached the same userspace by another route.
case "$BOOT_MODE" in
  sbi) SUCCESS_MARKERS+=("efi: UEFI not found.") ;;
  u-boot) SUCCESS_MARKERS+=("U-Boot ${UB_VERSION}" "Starting kernel ...") ;;
  edk2)
    # Limine prints literal backticks around paths.
    # shellcheck disable=SC2016
    SUCCESS_MARKERS+=("by EDK II" "RUSTSBI-LIMINE"
      'linux: Loading kernel `boot():/vmlinuz-linux`'
      'linux: Loading module `boot():/initramfs-linux.img`')
    ;;
esac
readonly SUCCESS_MARKERS

# Text that means the boot already went wrong. A panic halts the machine while
# QEMU stays alive, so without this the job would sit out the whole timeout.
# `!>` is how the tinyramfs initramfs reports a fatal error before it drops
# to an emergency shell, e.g. when it cannot find or mount the live medium.
# `[FAILED]` covers any dinit service that fails to start, including one that
# exceeds its start timeout.
readonly BOOT_FAILURE_PATTERN='PANIC|Kernel panic|not syncing|Attempted to kill init|!>|\[FAILED\]|System shutdown scheduled due to RustSBI panic'

readonly CACHE_DIR="${EWEOS_CACHE_DIR:-.cache/eweos}"
readonly WORK_DIR="${EWEOS_WORK_DIR:-.eweos/work}"
readonly LOG_DIR="${QEMU_LOG_DIR:-qemu-logs}"
readonly LOG_FILE="${LOG_DIR}/prototyper-eweos-${BOOT_MODE}.log"

# The sbi path boots the dynamic ELF directly; the bootloader paths boot the
# flat dynamic binary so the next stage can be placed at its load address.
if [[ "$BOOT_MODE" = sbi ]]; then
  readonly RUSTSBI="${EWEOS_RUSTSBI:-target/riscv64gc-unknown-none-elf/release/rustsbi-prototyper-dynamic.elf}"
  readonly BOOT_TIMEOUT_SECS="${EWEOS_BOOT_TIMEOUT_SECS:-420}"
else
  readonly RUSTSBI="${EWEOS_RUSTSBI:-target/riscv64gc-unknown-none-elf/release/rustsbi-prototyper-dynamic.bin}"
  readonly BOOT_TIMEOUT_SECS="${EWEOS_BOOT_TIMEOUT_SECS:-480}"
fi
readonly SMP="${EWEOS_SMP:-2}"

readonly ISO_PATH="${CACHE_DIR}/${ISO_NAME}"
readonly KERNEL_IMAGE="${WORK_DIR}/vmlinuz-linux"
readonly INITRAMFS="${WORK_DIR}/initramfs-linux.img"
readonly BOOT_DISK="${WORK_DIR}/${BOOT_MODE}-boot.img"

# U-Boot does not embed the firmware in S-mode, so its build is cached. The
# boot command is compiled in, so the workflow keys that cache on this script.
readonly UB_CACHE="${CACHE_DIR}/u-boot-${UB_VERSION}-smode"
readonly UB_BIN="${UB_CACHE}/u-boot.bin"

readonly QEMU_PREFIX="${CACHE_DIR}/qemu-${QEMU_VERSION}"
readonly QEMU="${EWEOS_QEMU:-${QEMU_PREFIX}/bin/qemu-system-riscv64}"

readonly EDK2_CACHE="${CACHE_DIR}/edk2-${EDK2_COMMIT}"
readonly EDK2_CODE="${EDK2_CACHE}/RISCV_VIRT_CODE.fd"
readonly EDK2_VARS_TEMPLATE="${EDK2_CACHE}/RISCV_VIRT_VARS.fd"
readonly EDK2_VARS="${WORK_DIR}/RISCV_VIRT_VARS.fd"

readonly DOWNLOAD_CONNECT_TIMEOUT_SECS="${EWEOS_DOWNLOAD_CONNECT_TIMEOUT_SECS:-30}"
readonly DOWNLOAD_TIMEOUT_SECS="${EWEOS_DOWNLOAD_TIMEOUT_SECS:-1800}"

QEMU_PID=""

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

  # The eweOS mirror has been seen to reset long transfers, so resume the
  # partial file on retry instead of starting the 800 MiB download over.
  if ! curl --fail --location \
    --connect-timeout "$DOWNLOAD_CONNECT_TIMEOUT_SECS" \
    --max-time "$DOWNLOAD_TIMEOUT_SECS" \
    --retry 5 \
    --retry-all-errors \
    --retry-max-time "$DOWNLOAD_TIMEOUT_SECS" \
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

# Verify a cached ISO without downloading anything. A digest mismatch is an
# error rather than a silent re-download, so a stale cache entry is visible.
check_cached_iso() {
  local actual

  if [[ ! -e "$ISO_PATH" ]]; then
    echo "No cached ${ISO_NAME}; the boot step will download it" >&2
    return
  fi

  actual=$(sha256sum "$ISO_PATH" | cut -d' ' -f1)
  if [[ "$actual" != "$ISO_SHA256" ]]; then
    echo "Cached ${ISO_PATH} does not match the pinned digest" >&2
    echo "expected ${ISO_SHA256}" >&2
    echo "actual   ${actual}" >&2
    return 1
  fi
  echo "Cached ${ISO_NAME} matches ${ISO_SHA256}" >&2
}

download_iso() {
  download_asset "$ISO_URL" "$ISO_PATH" "$ISO_SHA256" || {
    echo "The rolling ISO may have changed during this run; rerun to resolve its current checksum." >&2
    return 1
  }
}

# Build only the riscv64 system emulator from a pinned release. The install
# prefix is the only product cached.
prepare_qemu() {
  local tarball="${WORK_DIR}/qemu-${QEMU_VERSION}.tar.xz"
  local tree="${WORK_DIR}/qemu-${QEMU_VERSION}"
  local prefix

  if [[ -n "${EWEOS_QEMU:-}" ]]; then
    return
  fi
  if [[ -x "$QEMU" ]]; then
    echo "Using cached QEMU ${QEMU_VERSION}" >&2
    return
  fi

  prefix="$(realpath -m "$QEMU_PREFIX")"
  mkdir -p "$WORK_DIR"
  download_asset "$QEMU_URL" "$tarball" "$QEMU_SHA256"

  rm -rf "$tree" "$prefix"
  tar -xJf "$tarball" -C "$WORK_DIR"
  (
    cd "$tree"
    ./configure --prefix="$prefix" --target-list=riscv64-softmmu \
      --disable-docs --disable-user --disable-werror
    make -j"$(nproc)"
    make install
  )
  rm -rf "$tree"
}

# Pull out the kernel and initramfs that the ISO's Limine entry boots.
extract_kernel() {
  rm -f "$KERNEL_IMAGE" "$INITRAMFS"
  mkdir -p "$WORK_DIR"
  bsdtar -xf "$ISO_PATH" -C "$WORK_DIR" vmlinuz-linux initramfs-linux.img
  test -s "$KERNEL_IMAGE"
  test -s "$INITRAMFS"
}

# Build S-mode U-Boot with a compiled-in boot command that loads the ISO's
# kernel and initramfs from the FAT disk, which QEMU attaches first so that
# U-Boot numbers it virtio 0. `booti` edits the tree it is given in place
# (fdt_high is all ones), and editing U-Boot's own live control tree corrupts
# U-Boot before it reaches the kernel, so hand over a copy at fdt_addr_r. The
# single quotes keep the command line one hush word and keep the ${...}
# variables for U-Boot to expand at runtime; `scripts/config` rewrites the
# value with sed, so it must not contain `&`.
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

  make -C "$tree" O="$(realpath -m "$build")" ARCH=riscv CROSS_COMPILE="$CROSS_COMPILE" \
    qemu-riscv64_smode_defconfig
  "${tree}/scripts/config" --file "${build}/.config" --enable USE_BOOTCOMMAND
  # shellcheck disable=SC2016
  "${tree}/scripts/config" --file "${build}/.config" --set-str BOOTCOMMAND \
    "virtio scan; load virtio 0 \${kernel_addr_r} vmlinuz-linux; load virtio 0 \${ramdisk_addr_r} initramfs-linux.img; setenv bootargs '${KERNEL_CMDLINE}'; fdt move \${fdtcontroladdr} \${fdt_addr_r} 0x10000; booti \${kernel_addr_r} \${ramdisk_addr_r}:\${filesize} \${fdt_addr_r}"
  make -C "$tree" O="$(realpath -m "$build")" ARCH=riscv CROSS_COMPILE="$CROSS_COMPILE" \
    -j"$(nproc)"

  cp "${build}/u-boot.bin" "$UB_BIN"
}

# A whole-disk FAT filesystem written with mtools, so building the disk needs
# neither root nor loop devices. It is rebuilt every run from the files just
# extracted from the pinned ISO.
make_boot_disk() {
  rm -f "$BOOT_DISK"
  truncate -s 64M "$BOOT_DISK"
  mkfs.vfat -F 32 "$BOOT_DISK" >/dev/null
  mcopy -i "$BOOT_DISK" "$KERNEL_IMAGE" ::vmlinuz-linux
  mcopy -i "$BOOT_DISK" "$INITRAMFS" ::initramfs-linux.img
}

# Boot the ISO's own Limine executable through the UEFI removable-media path.
# Do not pass -kernel/-initrd to QEMU: only Limine can load Linux on this path.
make_limine_disk() {
  local root="${WORK_DIR}/limine"
  rm -rf "$root"
  mkdir -p "$root"
  bsdtar -xf "$ISO_PATH" -C "$root" EFI/BOOT/BOOTRISCV64.EFI
  test -s "${root}/EFI/BOOT/BOOTRISCV64.EFI"
  cat >"${root}/limine.conf" <<EOF
timeout: 1
serial: yes
graphics: no
verbose: yes
interface_branding: RUSTSBI-LIMINE
/eweOS
    protocol: linux
    path: boot():/vmlinuz-linux
    module_path: boot():/initramfs-linux.img
    cmdline: ${KERNEL_CMDLINE}
EOF
  rm -f "$BOOT_DISK"
  truncate -s 64M "$BOOT_DISK"
  mkfs.vfat -F 32 "$BOOT_DISK" >/dev/null
  mcopy -s -i "$BOOT_DISK" "${root}/EFI" ::
  mcopy -i "$BOOT_DISK" "${root}/limine.conf" ::limine.conf
  mcopy -i "$BOOT_DISK" "$KERNEL_IMAGE" ::vmlinuz-linux
  mcopy -i "$BOOT_DISK" "$INITRAMFS" ::initramfs-linux.img
}

# Build EDK II's RISC-V QEMU payload from an exact commit. The firmware volumes
# are the only EDK II products cached; the writable variables volume is copied
# for every boot so one run cannot contaminate the next cache restore.
prepare_edk2() {
  local workspace
  workspace="$(realpath -m "${WORK_DIR}/edk2-workspace")"
  local source="${workspace}/edk2"
  local output="${workspace}/Build/RiscVVirtQemu/RELEASE_GCC5/FV"
  local head

  if [[ $(stat -c %s "$EDK2_CODE" 2>/dev/null || true) == 33554432 &&
        $(stat -c %s "$EDK2_VARS_TEMPLATE" 2>/dev/null || true) == 33554432 ]]; then
    echo "Using cached EDK II ${EDK2_VERSION} firmware" >&2
    return
  fi

  rm -rf "$workspace" "$EDK2_CACHE"
  mkdir -p "$workspace" "$EDK2_CACHE"

  git init --quiet "$source"
  git -C "$source" remote add origin "$EDK2_URL"
  git -C "$source" fetch --quiet --depth=1 origin "$EDK2_COMMIT"
  git -C "$source" checkout --quiet --detach FETCH_HEAD
  head=$(git -C "$source" rev-parse HEAD)
  if [[ "$head" != "$EDK2_COMMIT" ]]; then
    echo "EDK II checkout mismatch: expected ${EDK2_COMMIT}, got ${head}" >&2
    return 1
  fi
  # EDK II's top-level submodules contain every source used by this platform.
  # Their own nested development/test submodules are not build inputs.
  git -C "$source" submodule update --init --depth=1

  (
    cd "$workspace"
    export WORKSPACE="$PWD"
    export PACKAGES_PATH="$source"
    export EDK_TOOLS_PATH="${source}/BaseTools"
    export GCC5_RISCV64_PREFIX="$CROSS_COMPILE"

    # edksetup and BaseTools deliberately communicate through environment
    # variables, so keep these commands in one subshell. edksetup also probes
    # optional unset variables, which is incompatible with this script's
    # `set -u`; disable nounset only while sourcing that upstream script.
    set +u
    # shellcheck disable=SC1091
    source "${source}/edksetup.sh" --reconfig
    set -u
    make -C "${source}/BaseTools" -j"$(nproc)"
    set +u
    # shellcheck disable=SC1091
    source "${source}/edksetup.sh" BaseTools
    set -u
    build -a RISCV64 -b RELEASE \
      -p OvmfPkg/RiscVVirt/RiscVVirtQemu.dsc \
      -t GCC5
  )

  cp "${output}/RISCV_VIRT_CODE.fd" "$EDK2_CODE"
  cp "${output}/RISCV_VIRT_VARS.fd" "$EDK2_VARS_TEMPLATE"
  truncate -s 32M "$EDK2_CODE" "$EDK2_VARS_TEMPLATE"
}

check_prerequisites() {
  test -s "$RUSTSBI" || {
    echo "Missing $RUSTSBI; run 'cargo prototyper build' first" >&2
    return 1
  }
  "$QEMU" --version
}

stop_qemu() {
  if [[ -n "$QEMU_PID" ]] && kill -0 "$QEMU_PID" 2>/dev/null; then
    kill "$QEMU_PID" 2>/dev/null || true
    wait "$QEMU_PID" 2>/dev/null || true
  fi
}

start_qemu() {
  local -a machine=(-machine virt)
  local -a boot=()
  local -a disks=()

  case "$BOOT_MODE" in
    sbi)
      boot=(-kernel "$KERNEL_IMAGE" -initrd "$INITRAMFS" -append "$KERNEL_CMDLINE")
      ;;
    u-boot)
      boot=(-kernel "$UB_BIN")
      # U-Boot numbers the disk given first on the command line virtio 0,
      # which the compiled-in boot command loads from, so the FAT disk goes
      # ahead of the ISO.
      disks=(
        -blockdev "node-name=boot,driver=file,read-only=on,filename=${BOOT_DISK}"
        -device "virtio-blk-device,drive=boot"
      )
      ;;
    edk2)
      cp "$EDK2_VARS_TEMPLATE" "$EDK2_VARS"
      machine=(-machine "virt,pflash0=pflash0,pflash1=pflash1,acpi=off")
      boot=(
        -blockdev "node-name=pflash0,driver=file,read-only=on,filename=${EDK2_CODE}"
        -blockdev "node-name=pflash1,driver=file,filename=${EDK2_VARS}"
      )
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
  "$QEMU" \
    "${machine[@]}" \
    -smp "$SMP" \
    -m 2G \
    -nographic \
    -no-reboot \
    -bios "$RUSTSBI" \
    "${boot[@]}" \
    "${disks[@]}" \
    -blockdev "node-name=iso,driver=file,read-only=on,filename=${ISO_PATH}" \
    -device virtio-blk-device,drive=iso \
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
  echo "eweOS failed to boot (${BOOT_MODE}):" >&2
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

  echo "QEMU exited before eweOS reached userspace (exit=${qemu_exit})" >&2
  tail -n 120 "$LOG_FILE" || true
}

wait_for_userspace() {
  local elapsed
  for ((elapsed = 0; elapsed < BOOT_TIMEOUT_SECS; elapsed++)); do
    # Failure is checked first: a service that fails before greetd starts
    # still leaves both success markers in the log.
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

  echo "eweOS did not reach userspace within ${BOOT_TIMEOUT_SECS}s" >&2
  tail -n 120 "$LOG_FILE" || true
  return 1
}

main() {
  if [[ "$BOOT_MODE" = check-iso ]]; then
    check_cached_iso
    return
  fi

  trap stop_qemu EXIT

  prepare_qemu
  check_prerequisites
  download_iso
  extract_kernel
  case "$BOOT_MODE" in
    u-boot)
      prepare_uboot
      make_boot_disk
      ;;
    edk2)
      prepare_edk2
      make_limine_disk
      ;;
    sbi) ;;
  esac

  start_qemu
  wait_for_userspace
  stop_qemu

  echo "RustSBI booted eweOS to userspace successfully (${BOOT_MODE})"
  echo "QEMU log: ${LOG_FILE}"
}

main "$@"
