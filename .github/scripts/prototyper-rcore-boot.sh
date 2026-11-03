#!/usr/bin/env bash
#
# Boot the pinned rCore to its BusyBox shell through RustSBI Prototyper.
# `sbi` boots the dynamic firmware directly; `u-boot` boots an SPL with
# the firmware embedded as its OpenSBI payload and `bootm`s rCore from a
# FAT disk. One test-guest-only patch redirects rCore's pinned riscv
# crate to a copy that keeps its forced A/D bits off non-leaf page table
# entries; see patch_guest_riscv_crate and the provenance log.

set -euo pipefail

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

cd "$(dirname "${BASH_SOURCE[0]}")/../.."

readonly RCORE_URL="https://github.com/rcore-os/rCore.git"
readonly RCORE_COMMIT="66cb4181ec6d3336d507c7c1ff100127f56fcc0a"
# Path dependency of the kernel build.
readonly RBOOT_URL="https://github.com/rcore-os/rboot.git"
readonly RBOOT_COMMIT="ea29a73dcf579fcf4215542423a60f75f4244d37"
# rCore pins this fork; the guest patch applies to it.
readonly RISCV_URL="https://github.com/rcore-riscv-hypervisor-dev/riscv.git"
readonly RISCV_REV="3f5efb1b8d4ceb12e291ef3d7b27120ea038eb83"
readonly GUEST_TOOLCHAIN="nightly-2020-06-04"

# Mutable release asset; pin its digest.
readonly PREBUILT_VERSION="0.1.2"
readonly PREBUILT_URL="https://github.com/rcore-os/rcore-user/releases/download/v${PREBUILT_VERSION}/riscv64.tar.gz"
readonly PREBUILT_SHA256="4b30f2d23ef60af6309bc04995666002bbc1aec4cc0784cbd885ceb8c37c5ad6"
readonly RCORE_FS_FUSE_REV="7f5eeac"

readonly UB_VERSION="2024.04"
readonly UB_URL="https://github.com/u-boot/u-boot/archive/refs/tags/v${UB_VERSION}.tar.gz"
readonly UB_SHA256="d6b57ce574a0a0504a5b6596644ceacb7f77bde9353779bcf2fde07c4b9a2b92"
readonly CROSS_COMPILE="riscv64-linux-gnu-"

readonly KERNEL_ENTRY_ADDR="0x80200000"
# The early boot page table maps only the first gigabyte of RAM.
readonly GUEST_MEMORY="512M"
readonly SMP_HARTS="1"
readonly SMOKE_MARKER="/ # "

# User-space page faults are normal lazy population; the rest are fatal.
readonly BOOT_FAILURE_PATTERN='panicked at|unhandled trap|Kernel panic|Unhandled exception|System shutdown scheduled due to RustSBI panic'

readonly DOWNLOAD_CONNECT_TIMEOUT_SECS="${RCORE_BOOT_DOWNLOAD_CONNECT_TIMEOUT_SECS:-30}"
readonly DOWNLOAD_TIMEOUT_SECS="${RCORE_BOOT_DOWNLOAD_TIMEOUT_SECS:-900}"
readonly BOOT_TIMEOUT_SECS="${RCORE_BOOT_TIMEOUT_SECS:-120}"

# The sbi path boots the dynamic firmware; the u-boot path embeds the
# flattened binary in the SPL.
if [[ "$BOOT_MODE" = u-boot ]]; then
  readonly RUSTSBI="${RCORE_BOOT_RUSTSBI_BIN:-target/riscv64gc-unknown-none-elf/release/rustsbi-prototyper.bin}"
else
  readonly RUSTSBI="${RCORE_BOOT_RUSTSBI:-target/riscv64gc-unknown-none-elf/release/rustsbi-prototyper-dynamic.bin}"
fi

readonly LOG_DIR="${QEMU_LOG_DIR:-qemu-logs}"
readonly LOG_FILE="${LOG_DIR}/prototyper-rcore-${BOOT_MODE}.log"
readonly PROVENANCE_FILE="${LOG_DIR}/prototyper-rcore-${BOOT_MODE}-provenance.log"

WORK_DIR="$(mktemp -d "${TMPDIR:-/tmp}/rustsbi-rcore.XXXXXX")"
readonly WORK_DIR
# Isolated cargo home: the guest's 2020 cargo cannot read sparse registry
# configurations. Override to reuse a warm registry locally.
CARGO_HOME_GUEST="${RCORE_BOOT_CARGO_HOME:-${WORK_DIR}/cargo-home}"
readonly CARGO_HOME_GUEST

QEMU_PID=""

# Mutable globals; the U-Boot tree is rebuilt every run.
UB_TREE=""
UB_SPL=""
UB_ITB=""

# Download an archive, verifying its digest on every run.
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

# Check out an exact commit, retrying transient fetch failures.
fetch_pinned() {
  local url=$1
  local commit=$2
  local destination=$3
  local attempt

  git init --quiet "$destination"
  git -C "$destination" remote add origin "$url"
  for attempt in 1 2 3 4 5; do
    if git -C "$destination" fetch --quiet --depth=1 origin "$commit"; then
      break
    fi
    (( attempt < 5 )) || return 1
    echo "git fetch failed (attempt ${attempt}), retrying in 10s" >&2
    sleep 10
  done
  test "$(git -C "$destination" rev-parse FETCH_HEAD)" = "$commit"
  git -C "$destination" checkout --quiet --detach FETCH_HEAD
}

# The crate forces the A/D bits on every entry, including the non-leaf
# pointers the spec reserves and QEMU rejects; keep them on leaf entries
# only, in this disposable build tree.
patch_guest_riscv_crate() {
  local tree=$1
  local source_path="${tree}/src/paging/page_table.rs"

  python3 - "$source_path" <<'PY_SHIM'
from pathlib import Path
import sys

p = Path(sys.argv[1])
s = p.read_text()
old = """    fn set<T: PhysicalAddress>(&mut self, frame: FrameWith<T>, mut flags: PageTableFlags) {
        // U540 will raise page fault when accessing page with A=0 or D=0
        flags |= EF::ACCESSED | EF::DIRTY;
        self.0 = ((frame.number() << 10) | flags.bits()) as u64;
    }"""
new = """    fn set<T: PhysicalAddress>(&mut self, frame: FrameWith<T>, mut flags: PageTableFlags) {
        // U540 will raise page fault when accessing page with A=0 or D=0.
        // TEST-GUEST-ONLY: only leaf entries may carry A/D; QEMU fails
        // the walk when a non-leaf pointer sets them.
        if flags.intersects(EF::READABLE | EF::WRITABLE | EF::EXECUTABLE) {
            flags |= EF::ACCESSED | EF::DIRTY;
        }
        self.0 = ((frame.number() << 10) | flags.bits()) as u64;
    }"""
if s.count(old) != 1:
    raise SystemExit('Pinned riscv crate does not contain the expected U540 A/D workaround')
p.write_text(s.replace(old, new))
print('Patched guest riscv crate: forced A/D on non-leaf PTEs disabled')
PY_SHIM
}

prepare_rcore() {
  local tree="${WORK_DIR}/rCore"

  fetch_pinned "$RCORE_URL" "$RCORE_COMMIT" "$tree"
  fetch_pinned "$RBOOT_URL" "$RBOOT_COMMIT" "${tree}/rboot"
  # Inside the checkout so the [patch] path below resolves.
  fetch_pinned "$RISCV_URL" "$RISCV_REV" "${tree}/riscv"
  patch_guest_riscv_crate "${tree}/riscv"

  # Redirect the riscv dependency to the patched copy.
  tee -a "${tree}/kernel/Cargo.toml" > /dev/null <<TOML_PATCH

# TEST-GUEST-ONLY: patched copy of the pinned riscv crate; see this
# script. Not an upstream rCore change.
[patch."https://github.com/rcore-riscv-hypervisor-dev/riscv"]
riscv = { path = "../riscv" }
TOML_PATCH

  # One value per --component flag; a space-separated list parses as a
  # toolchain name.
  rustup toolchain list | grep -q "^${GUEST_TOOLCHAIN}-" || \
    rustup toolchain install "$GUEST_TOOLCHAIN" --profile minimal \
      --component rust-src --component llvm-tools-preview

  # Retry the build: the guest cargo's crates.io index clone has no
  # retry, and the Makefile's libcore patch step is idempotent.
  local attempt
  for attempt in 1 2 3; do
    if make -C "${tree}/kernel" build ARCH=riscv64 MODE=release LOG=info \
      CARGO_HOME="$CARGO_HOME_GUEST" \
      CARGO_NET_GIT_FETCH_WITH_CLI=true; then
      break
    fi
    (( attempt < 3 )) || return 1
    echo "kernel build failed (attempt ${attempt}), retrying in 15s" >&2
    sleep 15
  done
  test -s "${tree}/kernel/target/riscv64/release/kernel.img"
}

prepare_user_image() {
  local tarball="${WORK_DIR}/rcore-user-riscv64.tar.gz"
  local tree="${WORK_DIR}/rcore-user"

  download_asset "$PREBUILT_URL" "$tarball" "$PREBUILT_SHA256"
  mkdir -p "$tree"
  tar -xzf "$tarball" -C "$tree"

  (
    # Outside the rCore tree so its rust-toolchain file does not pin
    # this host build.
    cd "${WORK_DIR}"
    CARGO_HOME="$CARGO_HOME_GUEST" cargo install --locked \
      rcore-fs-fuse --git https://github.com/rcore-os/rcore-fs \
      --rev "$RCORE_FS_FUSE_REV" --force --root "$CARGO_HOME_GUEST"
  )
  "${CARGO_HOME_GUEST}/bin/rcore-fs-fuse" \
    "${WORK_DIR}/riscv64.img" "${tree}/riscv64" zip
  qemu-img convert -f raw "${WORK_DIR}/riscv64.img" -O qcow2 "${WORK_DIR}/riscv64.qcow2"
  qemu-img resize "${WORK_DIR}/riscv64.qcow2" +1G > /dev/null
}

# Rebuilt every run: the SPL embeds the firmware under test.
build_uboot() {
  local tarball="${WORK_DIR}/u-boot-${UB_VERSION}.tar.gz"
  local tree="${WORK_DIR}/u-boot-${UB_VERSION}"
  local rustsbi_abs

  # OPENSBI must be absolute; make -C resolves it against the U-Boot tree.
  rustsbi_abs="$(readlink -f "$RUSTSBI")"

  download_asset "$UB_URL" "$tarball" "$UB_SHA256"
  rm -rf "$tree"
  tar -xzf "$tarball" -C "${WORK_DIR}"

  make -C "$tree" ARCH=riscv CROSS_COMPILE="$CROSS_COMPILE" \
    OPENSBI="$rustsbi_abs" qemu-riscv64_spl_defconfig > /dev/null

  # Move the control device tree to fdt_addr_r for rCore's a1. The FAT
  # disk is `virtio 1` (U-Boot numbers devices in QEMU command-line
  # order), and the value must not contain `&` because scripts/config
  # rewrites it with sed.
  "${tree}/scripts/config" --file "${tree}/.config" --enable USE_BOOTCOMMAND
  # shellcheck disable=SC2016
  "${tree}/scripts/config" --file "${tree}/.config" --set-str BOOTCOMMAND \
    "load virtio 1 \${kernel_addr_r} rcore.uimg; fdt addr \${fdtcontroladdr}; fdt move \${fdtcontroladdr} \${fdt_addr_r} 0x10000; bootm \${kernel_addr_r} - \${fdt_addr_r}"

  make -C "$tree" ARCH=riscv CROSS_COMPILE="$CROSS_COMPILE" \
    OPENSBI="$rustsbi_abs" -j"$(nproc)" > /dev/null

  UB_TREE="$tree"
  UB_SPL="${tree}/spl/u-boot-spl"
  UB_ITB="${tree}/u-boot.itb"
}

# Wrap the raw kernel in a uImage at 0x80200000; the FAT disk is written
# with mtools, needing neither root nor loop devices.
make_boot_disk() {
  local disk="${WORK_DIR}/rcore-boot.img"

  "${UB_TREE}/tools/mkimage" -A riscv -O linux -T kernel -C none \
    -a "$KERNEL_ENTRY_ADDR" -e "$KERNEL_ENTRY_ADDR" -n rCore \
    -d "${WORK_DIR}/rCore/kernel/target/riscv64/release/kernel.img" \
    "${WORK_DIR}/rcore.uimg" > /dev/null

  truncate -s 64M "$disk"
  mkfs.vfat -F 32 "$disk" > /dev/null
  mcopy -i "$disk" "${WORK_DIR}/rcore.uimg" ::rcore.uimg
}

check_prerequisites() {
  test -s "$RUSTSBI" || {
    echo "Missing $RUSTSBI; run 'cargo prototyper build' first" >&2
    return 1
  }
  qemu-system-riscv64 --version > /dev/null
}

stop_qemu() {
  if [[ -n "$QEMU_PID" ]] && kill -0 "$QEMU_PID" 2>/dev/null; then
    kill "$QEMU_PID" 2>/dev/null || true
    wait "$QEMU_PID" 2>/dev/null || true
  fi
}

start_qemu_sbi() {
  mkdir -p "$LOG_DIR"
  # Truncate stale logs so the wait loop cannot see an old marker.
  : >"$LOG_FILE"
  qemu-system-riscv64 \
    -machine virt \
    -smp "$SMP_HARTS" \
    -m "$GUEST_MEMORY" \
    -display none -serial stdio -monitor none -no-reboot \
    -bios "$RUSTSBI" \
    -kernel "${WORK_DIR}/rCore/kernel/target/riscv64/release/kernel.img" \
    -drive file="${WORK_DIR}/riscv64.qcow2",format=qcow2,id=hd0,if=none \
    -device virtio-blk-device,drive=hd0 \
    >"$LOG_FILE" 2>&1 </dev/null &
  QEMU_PID=$!
}

# The user image goes on the first virtio device, which rCore mounts as
# its root file system.
start_qemu_uboot() {
  mkdir -p "$LOG_DIR"
  : >"$LOG_FILE"
  qemu-system-riscv64 \
    -machine virt \
    -smp "$SMP_HARTS" \
    -m "$GUEST_MEMORY" \
    -display none -serial stdio -monitor none -no-reboot \
    -bios "$UB_SPL" \
    -device loader,file="$UB_ITB",addr="$KERNEL_ENTRY_ADDR" \
    -drive file="${WORK_DIR}/riscv64.qcow2",format=qcow2,id=hd0,if=none \
    -device virtio-blk-device,drive=hd0 \
    -drive file="${WORK_DIR}/rcore-boot.img",format=raw,id=hd1,if=none \
    -device virtio-blk-device,drive=hd1 \
    >"$LOG_FILE" 2>&1 </dev/null &
  QEMU_PID=$!
}

# Kernel banner, block device and shell prompt; the prompt may share a
# serial line with an ANSI reset of a kernel log record.
userspace_is_ready() {
  grep -Fq -- "${SMOKE_MARKER}" "$LOG_FILE" &&
    grep -Fq 'Hello RISCV! in hart 0' "$LOG_FILE" &&
    grep -Fq 'found a block device of size' "$LOG_FILE"
}

boot_has_failed() {
  grep -Eq "$BOOT_FAILURE_PATTERN" "$LOG_FILE"
}

report_boot_failure() {
  echo "rCore failed to boot:" >&2
  grep -E --max-count=5 "$BOOT_FAILURE_PATTERN" "$LOG_FILE" >&2 || true
  tail -n 120 "$LOG_FILE" || true
}

report_early_exit() {
  local qemu_exit
  set +e
  wait "$QEMU_PID"
  qemu_exit=$?
  set -e

  echo "QEMU exited before rCore reached userspace (exit=${qemu_exit})" >&2
  tail -n 120 "$LOG_FILE" || true
}

wait_for_userspace() {
  local elapsed
  for ((elapsed = 0; elapsed < BOOT_TIMEOUT_SECS; elapsed++)); do
    # A boot error must not be hidden by a userspace marker.
    if boot_has_failed; then
      report_boot_failure
      return 1
    fi
    if userspace_is_ready && ! boot_has_failed; then
      return 0
    fi
    if ! kill -0 "$QEMU_PID" 2>/dev/null; then
      if userspace_is_ready && ! boot_has_failed; then
        return 0
      fi
      report_early_exit
      return 1
    fi
    sleep 1
  done

  if userspace_is_ready && ! boot_has_failed; then
    return 0
  fi

  echo "rCore did not reach userspace within ${BOOT_TIMEOUT_SECS}s" >&2
  tail -n 120 "$LOG_FILE" || true
  return 1
}

write_provenance() {
  {
    printf 'boot_mode=%s\n' "$BOOT_MODE"
    printf 'source=rcore-os/rCore@%s\n' "$RCORE_COMMIT"
    printf 'rboot=rcore-os/rboot@%s\n' "$RBOOT_COMMIT"
    printf 'riscv_crate=rcore-riscv-hypervisor-dev/riscv@%s\n' "$RISCV_REV"
    printf 'guest_unmodified=false\n'
    printf 'guest_patch=riscv-crate-non-leaf-pte-ad-disabled\n'
    printf 'user_image=rcore-os/rcore-user v%s riscv64 (prebuilt release asset)\n' "$PREBUILT_VERSION"
    printf 'user_image_sha256=%s\n' "$PREBUILT_SHA256"
    printf 'rcore_fs_fuse_rev=%s\n' "$RCORE_FS_FUSE_REV"
    printf 'rust_toolchain=%s\n' "$GUEST_TOOLCHAIN"
    printf 'firmware=RustSBI-from-checked-out-revision (%s)\n' "$RUSTSBI"
    printf 'qemu=%s\n' "$(qemu-system-riscv64 --version | head -1)"
    sha256sum "$RUSTSBI" \
      "${WORK_DIR}/rCore/kernel/target/riscv64/release/kernel.img" \
      "${WORK_DIR}/riscv64.qcow2"
  } > "$PROVENANCE_FILE"
}

main() {
  # The EXIT trap performs both cleanups; signal traps only pick the
  # exit code.
  trap 'stop_qemu; rm -rf -- "$WORK_DIR"' EXIT
  trap 'exit 130' INT
  trap 'exit 143' TERM

  check_prerequisites
  prepare_rcore
  prepare_user_image
  case "$BOOT_MODE" in
    u-boot)
      build_uboot
      make_boot_disk
      start_qemu_uboot
      ;;
    sbi)
      start_qemu_sbi
      ;;
  esac
  write_provenance
  wait_for_userspace

  echo "RustSBI booted rCore ${RCORE_COMMIT:0:8} to userspace successfully (${BOOT_MODE})"
  echo "QEMU log: ${LOG_FILE}"
}

main "$@"
