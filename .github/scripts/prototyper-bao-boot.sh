#!/usr/bin/env bash
# Boot a single BusyBox Linux VM under Bao, with RustSBI Prototyper as M-mode
# firmware. Bao and lloader are rebuilt from pinned sources; immutable Linux
# and BusyBox products are cached because they dominate the build time.

set -euo pipefail

readonly BAO_COMMIT="0af4a1ab558ad60c9658af4f43756c4536dd3141"
readonly BAO_URL="https://github.com/bao-project/bao-hypervisor.git"
readonly BAO_DEMOS_COMMIT="f4d55d8044e1e17e6e54261e1da7db7e18d34725"
readonly BAO_DEMOS_URL="https://github.com/bao-project/bao-demos.git"

readonly KERNEL_VERSION="6.12.110"
readonly KERNEL_URL="https://cdn.kernel.org/pub/linux/kernel/v6.x/linux-${KERNEL_VERSION}.tar.xz"
readonly KERNEL_SHA256="8cee19e1839bb6ff4d5254d761933ae6ab670492d5ed030e09a80538320d5c4c"
readonly BUSYBOX_VERSION="1.36.1"
readonly BUSYBOX_URL="https://busybox.net/downloads/busybox-${BUSYBOX_VERSION}.tar.bz2"
readonly BUSYBOX_SHA256="b8cc24c9574d809e7279c3be349795c5d5ceb6fdf19ca709f80cde50e47de314"

readonly LINUX_CROSS_COMPILE="riscv64-linux-gnu-"
readonly BAREMETAL_CROSS_COMPILE="riscv64-unknown-elf-"
readonly SUCCESS_MARKER="RUSTSBI-BAO-LINUX-OK"
readonly FAILURE_PATTERN="BAO ERROR:|Kernel panic|not syncing|Attempted to kill init|panic|fatal|abort"

readonly REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
readonly INPUT_DIR="${REPO_ROOT}/.github/bao"
readonly CACHE_DIR="${BAO_CACHE_DIR:-${REPO_ROOT}/target/prototyper-bao/cache}"
readonly WORK_DIR="${BAO_WORK_DIR:-${REPO_ROOT}/target/prototyper-bao/work}"
readonly LOG_DIR="${QEMU_LOG_DIR:-${REPO_ROOT}/qemu-logs}"
readonly FIRMWARE_LOG="${LOG_DIR}/prototyper-bao-firmware.log"
readonly GUEST_LOG="${LOG_DIR}/prototyper-bao-guest.log"
readonly BOOT_TIMEOUT_SECS="${BAO_BOOT_TIMEOUT_SECS:-180}"
readonly DOWNLOAD_TIMEOUT_SECS="${BAO_DOWNLOAD_TIMEOUT_SECS:-900}"

readonly KERNEL_IMAGE="${CACHE_DIR}/linux-${KERNEL_VERSION}-bao-Image"
readonly BUSYBOX_INSTALL="${CACHE_DIR}/busybox-${BUSYBOX_VERSION}-install"
readonly ROOTFS="${WORK_DIR}/rootfs"
readonly LINUX_DTB="${WORK_DIR}/linux.dtb"
readonly LINUX_TARGET="${WORK_DIR}/linux"
readonly LINUX_BIN="${LINUX_TARGET}.bin"
readonly BAO_TREE="${WORK_DIR}/bao-hypervisor"
readonly DEMOS_TREE="${WORK_DIR}/bao-demos"
readonly BAO_BIN="${BAO_TREE}/bin/qemu-riscv64-virt/single-linux/bao.bin"
readonly RUSTSBI_ELF="${REPO_ROOT}/target/riscv64gc-unknown-none-elf/release/rustsbi-prototyper-payload.elf"

QEMU_PID=""

download_asset() {
  local url=$1 destination=$2 digest=$3 temp

  if [[ -f "$destination" ]] && printf '%s  %s\n' "$digest" "$destination" | sha256sum --check --status; then
    echo "Using cached $(basename "$destination")"
    return
  fi

  mkdir -p "$(dirname "$destination")"
  temp=$(mktemp "${destination}.part.XXXXXX")
  if ! curl --fail --location --connect-timeout 30 \
    --max-time "$DOWNLOAD_TIMEOUT_SECS" --retry 3 --retry-all-errors \
    --retry-max-time "$DOWNLOAD_TIMEOUT_SECS" --output "$temp" "$url"; then
    rm -f "$temp"
    return 1
  fi
  if ! printf '%s  %s\n' "$digest" "$temp" | sha256sum --check --status; then
    echo "Checksum mismatch for ${url}" >&2
    rm -f "$temp"
    return 1
  fi
  mv "$temp" "$destination"
}

checkout_commit() {
  local url=$1 commit=$2 tree=$3 head

  if [[ -d "${tree}/.git" ]] && head=$(git -C "$tree" rev-parse HEAD 2>/dev/null) && [[ "$head" = "$commit" ]]; then
    echo "Using cached checkout ${tree} at ${commit}"
    return
  fi

  rm -rf "$tree"
  git init --quiet "$tree"
  git -C "$tree" remote add origin "$url"
  git -C "$tree" fetch --quiet --depth=1 origin "$commit"
  git -C "$tree" checkout --quiet --detach FETCH_HEAD
  head=$(git -C "$tree" rev-parse HEAD)
  [[ "$head" = "$commit" ]] || {
    echo "Checkout mismatch: expected ${commit}, got ${head}" >&2
    return 1
  }
}

priv() {
  if [[ $EUID -eq 0 ]]; then
    "$@"
  else
    sudo "$@"
  fi
}

prepare_busybox() {
  local archive="${WORK_DIR}/busybox-${BUSYBOX_VERSION}.tar.bz2"
  local tree="${WORK_DIR}/busybox-${BUSYBOX_VERSION}"

  if [[ -x "${BUSYBOX_INSTALL}/bin/busybox" ]]; then
    echo "Using cached BusyBox ${BUSYBOX_VERSION}"
    return
  fi

  download_asset "$BUSYBOX_URL" "$archive" "$BUSYBOX_SHA256"
  rm -rf "$tree"
  tar -xjf "$archive" -C "$WORK_DIR"
  make -C "$tree" ARCH=riscv CROSS_COMPILE="$LINUX_CROSS_COMPILE" defconfig
  sed -i 's/^# CONFIG_STATIC is not set$/CONFIG_STATIC=y/' "${tree}/.config"
  grep -q '^CONFIG_STATIC=y' "${tree}/.config" || echo 'CONFIG_STATIC=y' >>"${tree}/.config"
  sed -i 's/^CONFIG_TC=y$/# CONFIG_TC is not set/' "${tree}/.config"
  grep -q '^# CONFIG_TC is not set$' "${tree}/.config" || echo '# CONFIG_TC is not set' >>"${tree}/.config"
  make -C "$tree" ARCH=riscv CROSS_COMPILE="$LINUX_CROSS_COMPILE" -j"$(nproc)"
  make -C "$tree" ARCH=riscv CROSS_COMPILE="$LINUX_CROSS_COMPILE" install
  rm -rf "$BUSYBOX_INSTALL"
  cp -a "${tree}/_install" "$BUSYBOX_INSTALL"
}

prepare_rootfs() {
  rm -rf "$ROOTFS"
  mkdir -p "$ROOTFS"
  cp -a "${BUSYBOX_INSTALL}/." "$ROOTFS/"
  mkdir -p "$ROOTFS/proc" "$ROOTFS/sys" "$ROOTFS/dev"
  cp "$INPUT_DIR/init" "$ROOTFS/init"
  chmod +x "$ROOTFS/init"
  priv mknod -m 600 "$ROOTFS/dev/console" c 5 1
  priv mknod -m 666 "$ROOTFS/dev/null" c 1 3
}

prepare_kernel() {
  local archive="${WORK_DIR}/linux-${KERNEL_VERSION}.tar.xz"
  local tree="${WORK_DIR}/linux-${KERNEL_VERSION}"
  local rootfs_abs

  if [[ -s "$KERNEL_IMAGE" ]]; then
    echo "Using cached Linux ${KERNEL_VERSION} Image"
    return
  fi

  download_asset "$KERNEL_URL" "$archive" "$KERNEL_SHA256"
  rm -rf "$tree"
  tar -xJf "$archive" -C "$WORK_DIR"
  make -C "$tree" ARCH=riscv CROSS_COMPILE="$LINUX_CROSS_COMPILE" defconfig
  rootfs_abs=$(realpath "$ROOTFS")
  "${tree}/scripts/config" --file "${tree}/.config" \
    --enable BLK_DEV_INITRD \
    --enable DEVTMPFS \
    --enable DEVTMPFS_MOUNT \
    --enable VIRTIO \
    --enable VIRTIO_MMIO \
    --enable VIRTIO_CONSOLE \
    --enable HVC_DRIVER \
    --set-str INITRAMFS_SOURCE "$rootfs_abs"
  make -C "$tree" ARCH=riscv CROSS_COMPILE="$LINUX_CROSS_COMPILE" olddefconfig
  make -C "$tree" ARCH=riscv CROSS_COMPILE="$LINUX_CROSS_COMPILE" -j"$(nproc)" Image
  cp "${tree}/arch/riscv/boot/Image" "$KERNEL_IMAGE"
}

build_linux_bin() {
  dtc -I dts -O dtb -o "$LINUX_DTB" "$INPUT_DIR/linux.dts"
  make -B -C "${DEMOS_TREE}/guests/linux/lloader" \
    ARCH=riscv64 CROSS_COMPILE="$BAREMETAL_CROSS_COMPILE" \
    IMAGE="$KERNEL_IMAGE" DTB="$LINUX_DTB" TARGET="$LINUX_TARGET"
}

build_bao() {
  local linux_abs
  linux_abs=$(realpath "$LINUX_BIN")
  make -B -C "$BAO_TREE" PLATFORM=qemu-riscv64-virt \
    CONFIG_REPO="$INPUT_DIR" CONFIG=single-linux \
    CROSS_COMPILE="$BAREMETAL_CROSS_COMPILE" \
    CPPFLAGS="-DBAO_LINUX_BIN=${linux_abs}" -j"$(nproc)"
}

build_rustsbi() {
  (cd "$REPO_ROOT" && cargo prototyper build --features hypervisor payload "$BAO_BIN")
}

stop_qemu() {
  local attempt qemu_state

  if [[ -n "$QEMU_PID" ]] && kill -0 "$QEMU_PID" 2>/dev/null; then
    kill "$QEMU_PID" 2>/dev/null || true
    # Give QEMU up to five seconds to handle SIGTERM. A dead child remains as
    # a zombie until wait reaps it, so treat Z as stopped instead of waiting
    # through the full grace period.
    for ((attempt = 0; attempt < 50; attempt++)); do
      if ! kill -0 "$QEMU_PID" 2>/dev/null; then
        break
      fi
      qemu_state=$(ps -o stat= -p "$QEMU_PID" 2>/dev/null || true)
      if [[ -z "$qemu_state" || "$qemu_state" = Z* ]]; then
        break
      fi
      sleep 0.1
    done
    if kill -0 "$QEMU_PID" 2>/dev/null; then
      qemu_state=$(ps -o stat= -p "$QEMU_PID" 2>/dev/null || true)
      if [[ -n "$qemu_state" && "$qemu_state" != Z* ]]; then
        kill -KILL "$QEMU_PID" 2>/dev/null || true
      fi
    fi
  fi
  if [[ -n "$QEMU_PID" ]]; then
    wait "$QEMU_PID" 2>/dev/null || true
    QEMU_PID=""
  fi
}

exit_with_signal_status() {
  local status=$1

  # Disable all traps before cleanup so the explicit exit cannot run cleanup
  # twice and another signal cannot interrupt the bounded reap sequence.
  trap - EXIT INT TERM
  stop_qemu
  exit "$status"
}

cleanup_on_exit() {
  local status=$?

  trap - EXIT
  stop_qemu
  exit "$status"
}

print_log_tails() {
  echo "===== RustSBI/Bao firmware log =====" >&2
  tail -n 120 "$FIRMWARE_LOG" >&2 || true
  echo "===== Bao Linux guest log =====" >&2
  tail -n 120 "$GUEST_LOG" >&2 || true
}

start_qemu() {
  mkdir -p "$LOG_DIR"
  : >"$FIRMWARE_LOG"
  : >"$GUEST_LOG"
  qemu-system-riscv64 \
    -M virt \
    -cpu rv64,sstc=true \
    -m 4G \
    -smp 4 \
    -nographic \
    -no-reboot \
    -bios "$RUSTSBI_ELF" \
    -device virtio-serial-device \
    -chardev "file,id=guest,path=${GUEST_LOG}" \
    -device virtconsole,chardev=guest \
    >"$FIRMWARE_LOG" 2>&1 &
  QEMU_PID=$!
}

wait_for_marker() {
  local elapsed qemu_exit

  for ((elapsed = 0; elapsed < BOOT_TIMEOUT_SECS; elapsed++)); do
    # Success wins once userspace prints the marker. In particular, Bao does
    # not currently offer a working guest poweroff mechanism, so any harmless
    # shutdown message after this point must not turn the run into a failure.
    if grep -Fq "$SUCCESS_MARKER" "$GUEST_LOG"; then
      stop_qemu
      echo "Bao Linux guest reached userspace in ${elapsed}s: ${SUCCESS_MARKER}"
      return 0
    fi
    if grep -Eiq "$FAILURE_PATTERN" "$FIRMWARE_LOG" "$GUEST_LOG"; then
      echo "Boot failure detected before ${SUCCESS_MARKER}" >&2
      print_log_tails
      return 1
    fi
    if ! kill -0 "$QEMU_PID" 2>/dev/null; then
      if grep -Fq "$SUCCESS_MARKER" "$GUEST_LOG"; then
        stop_qemu
        return 0
      fi
      set +e
      wait "$QEMU_PID"
      qemu_exit=$?
      set -e
      QEMU_PID=""
      echo "QEMU exited before ${SUCCESS_MARKER} (exit=${qemu_exit})" >&2
      print_log_tails
      return 1
    fi
    sleep 1
  done

  if grep -Fq "$SUCCESS_MARKER" "$GUEST_LOG"; then
    stop_qemu
    return 0
  fi
  echo "Timed out after ${BOOT_TIMEOUT_SECS}s waiting for ${SUCCESS_MARKER}" >&2
  print_log_tails
  return 1
}

main() {
  trap cleanup_on_exit EXIT
  trap 'exit_with_signal_status 130' INT
  trap 'exit_with_signal_status 143' TERM
  mkdir -p "$CACHE_DIR" "$WORK_DIR"

  qemu-system-riscv64 --version
  prepare_busybox
  prepare_rootfs
  prepare_kernel
  checkout_commit "$BAO_URL" "$BAO_COMMIT" "$BAO_TREE"
  checkout_commit "$BAO_DEMOS_URL" "$BAO_DEMOS_COMMIT" "$DEMOS_TREE"
  build_linux_bin
  build_bao
  build_rustsbi
  start_qemu
  wait_for_marker

  echo "Firmware log: ${FIRMWARE_LOG}"
  echo "Guest log: ${GUEST_LOG}"
}

main "$@"
