#!/usr/bin/env bash
#
# Boot hvisor, and a Linux guest under it, through RustSBI Prototyper in QEMU.
#
# RustSBI starts hvisor as its next stage. hvisor then brings up zone0, a Linux
# root zone booted from a pinned kernel, device tree and root filesystem, and
# zone1, a second Linux whose kernel, device tree and root filesystem ship
# inside that same root filesystem. zone1 reaching its shell is the milestone:
# it proves the firmware handed off, the hypervisor initialised, and a guest
# under the hypervisor runs userspace.
#
# hvisor is an HS-mode hypervisor, so the SBI it runs on has to expose the H
# extension; that is what the `hypervisor` feature turns on. The QEMU CPU needs
# `pmp=true` because the Prototyper programs PMP during boot and QEMU `virt`
# ships no PMP entries by default. hvisor's `qemu-plic` board asks for
# `aclint=on`, so the firmware also has to discover ACLINT.
#
# Console plumbing: zone0 owns the 16550 UART that `-nographic` exposes, so its
# shell is driven from this script's stdin and its output lands in the log.
# zone1's virtio-console backend runs inside zone0 and appears there as
# /dev/pts/<n>, so the script forwards that pty back into the same log and
# acknowledges zone1's prompt through it.
#
# hvisor and hvisor-tool are pinned and built here: the copies baked into the
# guest image predate the current hvisor and cannot talk to it. The toolchain is
# pinned to riscv-collab's Ubuntu 24.04 build on purpose — Ubuntu 26.04's
# default ISA includes the vector extension and the guest has no V, so a tool
# built with it takes an illegal instruction inside the guest.
#
# Requires: `cargo prototyper build --features hypervisor`, git, curl, unzip,
# xz, dtc, bc, make, sudo for the image loop mounts, and qemu-system-riscv64.

set -euo pipefail

if (( $# > 0 )); then
  echo "Usage: $0" >&2
  exit 2
fi

# hvisor, pinned to a commit that carries the DBCN console fix; without it the
# hypervisor is silent on any firmware that dropped the legacy console.
readonly HVISOR_VERSION="be9ba59"
readonly HVISOR_COMMIT="be9ba59f237b3ea06e3f8c489f0061b87d61032d"
readonly HVISOR_URL="https://github.com/syswonder/hvisor.git"

# hvisor-tool shares hvisor's `CONFIG_MAGIC_VERSION`, so both halves of the
# hypercall interface come from the same release line.
readonly HVISOR_TOOL_VERSION="ba997a4"
readonly HVISOR_TOOL_COMMIT="ba997a45f6552d1e49bb7d54ec81160aa33238db"
readonly HVISOR_TOOL_URL="https://github.com/syswonder/hvisor-tool.git"

# The zone0 kernel module is built against a prepared Linux tree; this one ships
# its build products, so it works as KDIR as-is.
readonly LINUX_VERSION="6.10-rc1"
readonly LINUX_COMMIT="84df15cbfcadaeffc602a6d36d33aa8fad8a0e31"
readonly LINUX_URL="https://codeload.github.com/CHonghaohao/linux_v6.10-rc1/tar.gz/refs/heads/main"

# Pinned for its non-vector baseline, not for its release date.
readonly TOOLCHAIN_VERSION="2026.08.27"
readonly TOOLCHAIN_URL="https://github.com/riscv-collab/riscv-gnu-toolchain/releases/download/${TOOLCHAIN_VERSION}/riscv64-glibc-ubuntu-24.04-gcc.tar.xz"

# Guest assets come from the release hvisor's own test scripts point at. Pin the
# bytes as well as the tag so a re-pushed asset cannot change the guest silently.
readonly ASSET_RELEASE="v2025.06.11"
readonly ASSET_URL="https://github.com/CHonghaohao/hvisor_env_img/releases/download/${ASSET_RELEASE}"
readonly ZONE0_KERNEL="Image"
readonly ZONE0_KERNEL_SHA256="2092ffb010578c35a4e4d77475330408f9667a8f61f41a704668b079ec3df0ac"
readonly ROOTFS_ARCHIVE="rootfs1.zip"
readonly ROOTFS_ARCHIVE_SHA256="ebaf9be18ee26b64a8d50302e8576418acf73f04324f9cba37f97c7bcfb84d24"
readonly ROOTFS_IMAGE="rootfs1.ext4"

# zone0 is a shell as pid 1, so its first prompt marks the hand-off to userspace.
readonly ZONE0_PROMPT='^# '
# The zone list row is printed by hvisor, not echoed from the command line.
readonly ZONE1_RUNNING_PATTERN='linux2.*running'
# Split so the smoke marker cannot be satisfied by the command line that sends it.
readonly SMOKE_MARKER='HVISOR-ZONE1-OK'
readonly BOOT_FAILURE_PATTERN='panic occurred|Kernel panic|not syncing|unhandled mmio fault|config versions mismatch|Failed to open /dev/hvisor|Attempted to kill init|No working init found|SBI panic'

readonly RUSTSBI="${HVISOR_RUSTSBI:-target/riscv64gc-unknown-none-elf/release/rustsbi-prototyper-dynamic.elf}"
readonly QEMU="${HVISOR_QEMU:-qemu-system-riscv64}"

# Absolute, because the build steps run `make -C` from a different directory.
CACHE_DIR="$(realpath -m "${HVISOR_CACHE_DIR:-.cache/hvisor}")"
readonly CACHE_DIR
WORK_DIR="$(realpath -m "${HVISOR_WORK_DIR:-${RUNNER_TEMP:-/tmp}/rustsbi-hvisor}")"
readonly WORK_DIR
readonly LOG_DIR="${QEMU_LOG_DIR:-qemu-logs}"
readonly LOG_FILE="${LOG_DIR}/prototyper-hvisor.log"
readonly BOOT_TIMEOUT_SECS="${HVISOR_BOOT_TIMEOUT_SECS:-300}"

readonly TOOLCHAIN_DIR="${CACHE_DIR}/toolchain"
readonly LINUX_SRC="${CACHE_DIR}/linux"
readonly GUEST_DIR="${CACHE_DIR}/guest"
readonly BUILT_DIR="${CACHE_DIR}/${HVISOR_VERSION}-tool-${HVISOR_TOOL_VERSION}"

readonly HVISOR_SRC="${WORK_DIR}/hvisor"
readonly HVISOR_TOOL_SRC="${WORK_DIR}/hvisor-tool"

readonly HVISOR_BIN="${BUILT_DIR}/hvisor.bin"
readonly ZONE0_DTB="${BUILT_DIR}/zone0.dtb"
readonly ZONE1_DTB="${BUILT_DIR}/zone1-linux.dtb"
readonly HVISOR_TOOL_BIN="${BUILT_DIR}/hvisor"
readonly HVISOR_TOOL_KO="${BUILT_DIR}/hvisor.ko"
readonly HVISOR_CONFIGS="${BUILT_DIR}/configs"

readonly ROOTFS_IMAGE_PATH="${GUEST_DIR}/${ROOTFS_IMAGE}"

QEMU_PID=""

check_prerequisites() {
  test -s "$RUSTSBI" || {
    echo "Missing $RUSTSBI; run 'cargo prototyper build --features hypervisor' first" >&2
    return 1
  }
  local tool
  for tool in git curl unzip xz dtc bc make python3 sudo; do
    command -v "$tool" >/dev/null || {
      echo "$tool is required to build hvisor and prepare the guest" >&2
      return 1
    }
  done
  python3 -c 'import venv' 2>/dev/null || {
    echo "python3 venv support is required for the hvisor kconfig step" >&2
    return 1
  }
  "$QEMU" --version
  if ! "$QEMU" -cpu help | awk '$1 == "rva23s64" { found = 1 } END { exit !found }'; then
    echo "qemu-system-riscv64 does not provide the required rva23s64 CPU" >&2
    return 1
  fi
}

git_checkout() {
  local url=$1
  local commit=$2
  local destination=$3

  if [[ ! -d "${destination}/.git" ]]; then
    git clone --quiet "$url" "$destination"
  fi
  # GitHub will not serve an arbitrary commit by id, so fetch the remote and
  # then select the pinned commit locally.
  git -C "$destination" fetch --quiet origin
  git -C "$destination" checkout --quiet --detach "$commit"
  [[ "$(git -C "$destination" rev-parse HEAD)" == "$commit" ]] || {
    echo "checkout mismatch in ${destination}: expected ${commit}" >&2
    return 1
  }
}

download_asset() {
  local url=$1
  local destination=$2
  local sha256=${3:-}
  local temporary

  if [[ -s "$destination" ]]; then
    if [[ -z "$sha256" ]] || verify_sha256 "$sha256" "$destination"; then
      return
    fi
  fi

  temporary=$(mktemp "${destination}.part.XXXXXX")
  echo "Downloading ${url}" >&2
  curl --fail --location --retry 3 --retry-all-errors \
    --connect-timeout 30 --max-time 3600 \
    --output "$temporary" "$url"
  if [[ -n "$sha256" ]]; then
    verify_sha256 "$sha256" "$temporary"
  fi
  mv "$temporary" "$destination"
}

verify_sha256() {
  local digest=$1
  local path=$2
  printf '%s  %s\n' "$digest" "$(basename "$path")" \
    | (cd "$(dirname "$path")" && sha256sum --check --status)
}

# The toolchain is a build input, so it is cached rather than rebuilt.
prepare_toolchain() {
  mkdir -p "$CACHE_DIR"
  if [[ -x "${TOOLCHAIN_DIR}/bin/riscv64-unknown-linux-gnu-gcc" ]]; then
    return
  fi
  local archive="${CACHE_DIR}/toolchain.tar.xz"
  download_asset "$TOOLCHAIN_URL" "$archive"
  mkdir -p "$TOOLCHAIN_DIR"
  tar -xJf "$archive" -C "$TOOLCHAIN_DIR" --strip-components=1
}

prepare_linux() {
  if [[ -s "${LINUX_SRC}/Module.symvers" && -s "${LINUX_SRC}/.config" ]]; then
    return
  fi
  local archive="${CACHE_DIR}/linux.tar.gz"
  download_asset "$LINUX_URL" "$archive"
  mkdir -p "$LINUX_SRC"
  tar -xzf "$archive" -C "$LINUX_SRC" --strip-components=1
  test -s "${LINUX_SRC}/Module.symvers" || {
    echo "Linux ${LINUX_VERSION} (${LINUX_COMMIT}) has no Module.symvers; it cannot build modules" >&2
    return 1
  }
}

prepare_guest() {
  mkdir -p "$GUEST_DIR"
  download_asset "${ASSET_URL}/${ZONE0_KERNEL}" \
    "${GUEST_DIR}/${ZONE0_KERNEL}" "$ZONE0_KERNEL_SHA256"
  download_asset "${ASSET_URL}/${ROOTFS_ARCHIVE}" \
    "${GUEST_DIR}/${ROOTFS_ARCHIVE}" "$ROOTFS_ARCHIVE_SHA256"
  if [[ ! -s "$ROOTFS_IMAGE_PATH" ]]; then
    unzip -o "${GUEST_DIR}/${ROOTFS_ARCHIVE}" -d "$GUEST_DIR" >/dev/null
  fi
  test -s "$ROOTFS_IMAGE_PATH"
}

# hvisor and its tools are built from the pinned commits with the pinned
# toolchain; hvisor selects that toolchain from its own rust-toolchain.toml.
build_hvisor() {
  mkdir -p "$WORK_DIR" "$BUILT_DIR"
  git_checkout "$HVISOR_URL" "$HVISOR_COMMIT" "$HVISOR_SRC"

  make -C "$HVISOR_SRC" all BID=riscv64/qemu-plic MODE=release
  make -C "$HVISOR_SRC" BID=riscv64/qemu-plic dtb

  install -m 0644 \
    "$HVISOR_SRC/target/riscv64gc-unknown-none-elf/release/hvisor.bin" \
    "$HVISOR_BIN"
  install -m 0644 \
    "$HVISOR_SRC/platform/riscv64/qemu-plic/image/dts/zone0.dtb" \
    "$ZONE0_DTB"
  install -m 0644 \
    "$HVISOR_SRC/platform/riscv64/qemu-plic/image/dts/zone1-linux.dtb" \
    "$ZONE1_DTB"
  install -d "$HVISOR_CONFIGS"
  install -m 0644 "$HVISOR_SRC"/platform/riscv64/qemu-plic/configs/*.json \
    "$HVISOR_CONFIGS/"
}

build_hvisor_tool() {
  mkdir -p "$WORK_DIR"
  git_checkout "$HVISOR_TOOL_URL" "$HVISOR_TOOL_COMMIT" "$HVISOR_TOOL_SRC"

  local toolchain_path="${TOOLCHAIN_DIR}/bin"
  local common_args=(ARCH=riscv LOG=LOG_INFO)
  # hvisor-tool expects a Debian-style prefix, which this toolchain does not use.
  common_args+=(CROSS_COMPILE=riscv64-unknown-linux-gnu-)
  # Newer GCC rejects a pre-existing prototype mismatch in the tool.
  common_args+=("CC=riscv64-unknown-linux-gnu-gcc -Wno-error=incompatible-pointer-types")

  make -C "$HVISOR_TOOL_SRC" clean "${common_args[@]}" >/dev/null 2>&1 || true
  PATH="${toolchain_path}:${PATH}" make -C "$HVISOR_TOOL_SRC" tools "${common_args[@]}"
  PATH="${toolchain_path}:${PATH}" make -C "$HVISOR_TOOL_SRC" driver "${common_args[@]}" \
    KDIR="$LINUX_SRC"

  install -m 0755 "${HVISOR_TOOL_SRC}/output/hvisor" "$HVISOR_TOOL_BIN"
  install -m 0644 "${HVISOR_TOOL_SRC}/output/hvisor.ko" "$HVISOR_TOOL_KO"
}

# zone1's kernel, device tree and configuration live inside zone0's root
# filesystem. The image's own copies predate the current hvisor and place zone1
# inside the region hvisor reserves for itself, so the pinned build replaces
# them; the backend also expects the zone1 disk under a different file name.
deploy_guest() {
  local mount_dir="${WORK_DIR}/rootfs"
  mkdir -p "$mount_dir"

  sudo mount "$ROOTFS_IMAGE_PATH" "$mount_dir"
  # shellcheck disable=SC2064
  trap "sudo umount '$mount_dir' || true" RETURN

  local guest_root="${mount_dir}/home/riscv64"
  test -d "$guest_root" || {
    echo "guest image has no /home/riscv64" >&2
    return 1
  }

  sudo install -m 0755 "$HVISOR_TOOL_BIN" "${guest_root}/hvisor"
  sudo install -m 0644 "$HVISOR_TOOL_KO" "${guest_root}/hvisor.ko"
  sudo install -m 0644 "$ZONE1_DTB" "${guest_root}/zone1-linux.dtb"
  sudo install -m 0644 "$HVISOR_CONFIGS"/*.json "$guest_root"/
  sudo ln -sf riscv_rootfs2.img "${guest_root}/rootfs2.ext4"
  sudo sync

  sudo umount "$mount_dir"
  trap - RETURN
}

stop_qemu() {
  if [[ -n "$QEMU_PID" ]] && kill -0 "$QEMU_PID" 2>/dev/null; then
    kill "$QEMU_PID" 2>/dev/null || true
    wait "$QEMU_PID" 2>/dev/null || true
  fi
}

# Wait for a prompt before answering it; sending earlier could drop input before
# the console that should receive it is even open.
wait_for() {
  local pattern=$1
  local limit=$2
  local i

  for ((i = 0; i < limit; i++)); do
    grep -aEq "$pattern" "$LOG_FILE" 2>/dev/null && return 0
    sleep 1
  done
  echo "[feed] never saw '${pattern}'" >&2
  return 1
}

# zone0's shell gets the setup commands; zone1 is then started and its own
# console, forwarded into the log, is acknowledged with a smoke command.
feed_console() {
  wait_for "$ZONE0_PROMPT" 180 || return 1
  printf 'cd /home/riscv64\r'
  printf 'insmod hvisor.ko\r'
  printf 'mkdir -p /dev/pts; mount -t devpts devpts /dev/pts\r'
  printf 'nohup ./hvisor virtio start virtio-backend.json > virtio.log 2>&1 &\r'
  sleep 3

  printf './hvisor zone start zone1-linux.json\r'
  sleep 5
  printf './hvisor zone list\r'
  wait_for "$ZONE1_RUNNING_PATTERN" 120 || return 1

  # zone1's console is a pty in zone0; reading it puts zone1's boot into the log.
  printf 'cat /dev/pts/0 &\r'
  sleep 5
  # Split the marker so the command line that sends it cannot satisfy the check.
  printf '%s\r' "printf 'echo -n HVISOR-; echo ZONE1-OK\\r' > /dev/pts/0"
}

userspace_is_ready() {
  grep -aFq "$SMOKE_MARKER" "$LOG_FILE" \
    && grep -aEq "$ZONE1_RUNNING_PATTERN" "$LOG_FILE"
}

boot_has_failed() {
  grep -aEq "$BOOT_FAILURE_PATTERN" "$LOG_FILE"
}

report_boot_failure() {
  echo "hvisor failed to boot:" >&2
  grep -a -E --max-count=5 "$BOOT_FAILURE_PATTERN" "$LOG_FILE" >&2 || true
  tail -n 120 "$LOG_FILE" >&2 || true
}

report_early_exit() {
  local qemu_exit
  set +e
  wait "$QEMU_PID"
  qemu_exit=$?
  set -e

  echo "QEMU exited before zone1 reached userspace (exit=${qemu_exit})" >&2
  tail -n 120 "$LOG_FILE" >&2 || true
}

start_qemu() {
  mkdir -p "$LOG_DIR"
  # Truncate any stale log from a previous run before QEMU starts, so the wait
  # loop cannot mistake an old success marker for this boot's.
  : >"$LOG_FILE"
  feed_console | "$QEMU" \
    -machine virt,aclint=on \
    -cpu rva23s64,pmp=true \
    -smp 4 \
    -m 4G \
    -nographic \
    -no-reboot \
    -bios "$RUSTSBI" \
    -kernel "$HVISOR_BIN" \
    -device "loader,file=${GUEST_DIR}/${ZONE0_KERNEL},addr=0x90000000,force-raw=on" \
    -device "loader,file=${ZONE0_DTB},addr=0x8f000000,force-raw=on" \
    -drive "if=none,file=${ROOTFS_IMAGE_PATH},id=hd0,format=raw" \
    -device virtio-blk-pci,drive=hd0,disable-legacy=on,disable-modern=off,addr=01.0 \
    >"$LOG_FILE" 2>&1 &
  QEMU_PID=$!
}

wait_for_userspace() {
  local elapsed
  for ((elapsed = 0; elapsed < BOOT_TIMEOUT_SECS; elapsed++)); do
    # Failure is checked first: a boot error after the shell appeared would
    # still leave the success marker in the log.
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

  echo "zone1 did not reach userspace within ${BOOT_TIMEOUT_SECS}s" >&2
  tail -n 120 "$LOG_FILE" >&2 || true
  return 1
}

main() {
  trap stop_qemu EXIT

  check_prerequisites
  prepare_toolchain
  prepare_linux
  prepare_guest
  # Only the build products are cached, so a warm cache skips both builds.
  if [[ -s "$HVISOR_BIN" && -s "$ZONE0_DTB" && -s "$ZONE1_DTB" \
        && -s "$HVISOR_TOOL_BIN" && -s "$HVISOR_TOOL_KO" \
        && -d "$HVISOR_CONFIGS" ]]; then
    echo "Using cached hvisor ${HVISOR_VERSION} and hvisor-tool ${HVISOR_TOOL_VERSION}" >&2
  else
    build_hvisor
    build_hvisor_tool
  fi
  deploy_guest

  start_qemu
  wait_for_userspace
  stop_qemu

  echo "RustSBI booted hvisor ${HVISOR_VERSION} to a zone1 Linux shell successfully"
  echo "QEMU log: ${LOG_FILE}"
}

main "$@"
