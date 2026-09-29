#!/usr/bin/env bash
#
# Exercise RustSBI's hypervisor support with a nested RISC-V virtual machine:
#
#   RustSBI -> KVM-enabled Linux -> Cloud Hypervisor -> minimal Linux guest
#
set -euo pipefail

if (( $# != 0 )); then
  echo "Usage: $0" >&2
  exit 2
fi

readonly SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
readonly REPO_ROOT="$(cd -- "${SCRIPT_DIR}/../.." && pwd)"
cd "$REPO_ROOT"

readonly LINUX_COMMIT="46b5aab6f24a7e31a861d66dfe9b559b310a6c2d"
readonly LINUX_REF="refs/heads/ch-6.12.8"
readonly LINUX_URL="https://github.com/cloud-hypervisor/linux.git"

readonly CLOUD_HYPERVISOR_VERSION="53.0"
readonly CLOUD_HYPERVISOR_COMMIT="9ed824d6d08df3e96f7d5f50795d9449ac99f431"
readonly CLOUD_HYPERVISOR_REF="refs/tags/v${CLOUD_HYPERVISOR_VERSION}"
readonly CLOUD_HYPERVISOR_URL="https://github.com/cloud-hypervisor/cloud-hypervisor.git"

readonly BUSYBOX_VERSION="1.36.1"
readonly BUSYBOX_URL="https://busybox.net/downloads/busybox-${BUSYBOX_VERSION}.tar.bz2"
readonly BUSYBOX_SHA256="b8cc24c9574d809e7279c3be349795c5d5ceb6fdf19ca709f80cde50e47de314"

readonly QEMU_VERSION="10.2.2"
readonly QEMU_URL="https://download.qemu.org/qemu-${QEMU_VERSION}.tar.xz"
readonly QEMU_SHA256="784b296ff29c1417aa72323abcb2d2ea9ab9771724f577dcd785c3b04f21e176"

readonly CROSS_COMPILE="riscv64-linux-gnu-"
readonly RUST_TARGET="riscv64gc-unknown-linux-gnu"
readonly RISCV_SYSROOT="${CLOUD_HYPERVISOR_RISCV_SYSROOT:-/usr/riscv64-linux-gnu}"

readonly HOST_MARKER="RUSTSBI-KVM-HOST-OK"
readonly GUEST_MARKER="RUSTSBI-CLOUD-HYPERVISOR-GUEST-OK"
readonly BOOT_FAILURE_PATTERN="Kernel panic|not syncing|Attempted to kill init|Failed to create VM"
readonly CACHE_RECIPE_VERSION="2"

readonly CACHE_DIR="${CLOUD_HYPERVISOR_CACHE_DIR:-.cache/cloud-hypervisor}"
readonly WORK_DIR="${CLOUD_HYPERVISOR_WORK_DIR:-target/cloud-hypervisor/work}"
readonly QEMU_PREFIX="${CLOUD_HYPERVISOR_QEMU_PREFIX:-.cache/qemu-${QEMU_VERSION}}"
readonly LOG_DIR="${QEMU_LOG_DIR:-qemu-logs}"
readonly LOG_FILE="${LOG_DIR}/prototyper-cloud-hypervisor.log"
readonly BOOT_TIMEOUT_SECS="${CLOUD_HYPERVISOR_BOOT_TIMEOUT_SECS:-900}"
readonly DOWNLOAD_CONNECT_TIMEOUT_SECS="${CLOUD_HYPERVISOR_DOWNLOAD_CONNECT_TIMEOUT_SECS:-30}"
readonly DOWNLOAD_TIMEOUT_SECS="${CLOUD_HYPERVISOR_DOWNLOAD_TIMEOUT_SECS:-900}"

readonly RUSTSBI="${CLOUD_HYPERVISOR_RUSTSBI:-target/riscv64gc-unknown-none-elf/release/rustsbi-prototyper-dynamic.elf}"
readonly KERNEL_IMAGE="${CACHE_DIR}/linux-${LINUX_COMMIT}-Image"
readonly KERNEL_STAMP="${KERNEL_IMAGE}.stamp"
readonly BUSYBOX_INSTALL="${CACHE_DIR}/busybox-${BUSYBOX_VERSION}-install"
readonly BUSYBOX_STAMP="${CACHE_DIR}/busybox-${BUSYBOX_VERSION}.stamp"
readonly CLOUD_HYPERVISOR_BIN="${CACHE_DIR}/cloud-hypervisor-${CLOUD_HYPERVISOR_COMMIT}"
readonly CLOUD_HYPERVISOR_STAMP="${CLOUD_HYPERVISOR_BIN}.stamp"
readonly GUEST_INITRAMFS="${WORK_DIR}/guest-initramfs.cpio.gz"
readonly HOST_INITRAMFS="${WORK_DIR}/host-initramfs.cpio.gz"

QEMU_PID=""
QEMU=""
SCRIPT_SHA256=""

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

kernel_stamp_metadata() {
  printf '%s\n' \
    "recipe=${CACHE_RECIPE_VERSION}" \
    "component=linux" \
    "source=${LINUX_COMMIT}" \
    "config=ch_defconfig" \
    "cross_gcc=$(${CROSS_COMPILE}gcc --version | sed -n '1p')" \
    "script_sha256=${SCRIPT_SHA256}"
}

busybox_stamp_metadata() {
  printf '%s\n' \
    "recipe=${CACHE_RECIPE_VERSION}" \
    "component=busybox" \
    "source=${BUSYBOX_SHA256}" \
    "config=defconfig+static-tc-disabled" \
    "cross_gcc=$(${CROSS_COMPILE}gcc --version | sed -n '1p')" \
    "script_sha256=${SCRIPT_SHA256}"
}

cloud_hypervisor_stamp_metadata() {
  local libc_digest
  libc_digest=$(sha256sum "${RISCV_SYSROOT}/lib/libc.so.6")
  libc_digest=${libc_digest%% *}
  printf '%s\n' \
    "recipe=${CACHE_RECIPE_VERSION}" \
    "component=cloud-hypervisor" \
    "source=${CLOUD_HYPERVISOR_COMMIT}" \
    "rustc=$(rustc --version)" \
    "cargo=$(cargo --version)" \
    "cross_gcc=$(${CROSS_COMPILE}gcc --version | sed -n '1p')" \
    "sysroot_libc_sha256=${libc_digest}" \
    "script_sha256=${SCRIPT_SHA256}"
}

artifact_cache_valid() {
  local artifact=$1
  local stamp=$2
  local expected_metadata=$3
  local stored_metadata stored_digest actual_digest

  [[ -s "$artifact" && -s "$stamp" ]] || return 1
  stored_metadata=$(sed '/^artifact_sha256=/d' "$stamp")
  [[ "$stored_metadata" == "$expected_metadata" ]] || return 1
  stored_digest=$(sed -n 's/^artifact_sha256=//p' "$stamp")
  [[ -n "$stored_digest" ]] || return 1
  actual_digest=$(sha256sum "$artifact")
  actual_digest=${actual_digest%% *}
  [[ "$actual_digest" == "$stored_digest" ]]
}

write_artifact_stamp() {
  local artifact=$1
  local stamp=$2
  local metadata=$3
  local digest temp

  digest=$(sha256sum "$artifact")
  digest=${digest%% *}
  temp=$(mktemp "${stamp}.part.XXXXXX")
  printf '%s\nartifact_sha256=%s\n' "$metadata" "$digest" >"$temp"
  mv "$temp" "$stamp"
}

checkout_commit() {
  local url=$1
  local commit=$2
  local ref=$3
  local tree=$4
  local head

  rm -rf "$tree"
  git init --quiet "$tree"
  git -C "$tree" remote add origin "$url"
  if ! git -C "$tree" fetch --quiet --depth=1 origin "$commit"; then
    echo "Fetching ${commit} by object ID failed; retrying advertised ref ${ref}" >&2
    git -C "$tree" fetch --quiet --depth=1 origin "$ref"
  fi
  git -C "$tree" checkout --quiet --detach FETCH_HEAD
  head=$(git -C "$tree" rev-parse HEAD)
  if [[ "$head" != "$commit" ]]; then
    echo "Checkout mismatch: expected ${commit}, got ${head}" >&2
    return 1
  fi
}

qemu_version() {
  "$1" --version | sed -n '1s/.*version \([0-9][0-9.]*\).*/\1/p'
}

qemu_is_usable() {
  local version major minor

  version=$(qemu_version "$1")
  IFS=. read -r major minor _ <<<"$version"
  [[ -n "$major" && -n "$minor" ]] &&
    (( major > 9 || (major == 9 && minor >= 2) ))
}

prepare_qemu() {
  local prefix system_qemu
  local tarball="${WORK_DIR}/qemu-${QEMU_VERSION}.tar.xz"
  local tree="${WORK_DIR}/qemu-${QEMU_VERSION}"

  if [[ -n "${CLOUD_HYPERVISOR_QEMU:-}" ]]; then
    QEMU=$CLOUD_HYPERVISOR_QEMU
  else
    system_qemu=$(command -v qemu-system-riscv64 || true)
    if [[ -n "$system_qemu" ]] && qemu_is_usable "$system_qemu"; then
      QEMU=$system_qemu
    else
      require_command ninja
      require_command pkg-config
      require_command python3
      require_command realpath
      prefix=$(realpath -m "$QEMU_PREFIX")
      QEMU="${prefix}/bin/qemu-system-riscv64"
      if [[ ! -x "$QEMU" ]]; then
        mkdir -p "$WORK_DIR"
        download_asset "$QEMU_URL" "$tarball" "$QEMU_SHA256"
        rm -rf "$tree" "$prefix"
        tar -xJf "$tarball" -C "$WORK_DIR"
        (
          cd "$tree"
          ./configure \
            --prefix="$prefix" \
            --target-list=riscv64-softmmu \
            --disable-docs \
            --disable-user \
            --disable-werror
          make -j"$(nproc)"
          make install
        )
        rm -rf "$tree"
      fi
    fi
  fi

  if [[ ! -x "$QEMU" ]] || ! qemu_is_usable "$QEMU"; then
    echo "QEMU >= 9.2 is required for the RISC-V AIA setup: ${QEMU}" >&2
    return 1
  fi
  "$QEMU" --version
}

prepare_kernel() {
  local tree="${WORK_DIR}/linux-${LINUX_COMMIT}"
  local stamp_metadata
  local option
  local -a required_options=(
    BLK_DEV_INITRD
    DEVTMPFS
    HVC_DRIVER
    KVM
    RISCV_APLIC
    RISCV_IMSIC
    SERIAL_8250_CONSOLE
    VIRTIO
    VIRTIO_CONSOLE
  )

  stamp_metadata=$(kernel_stamp_metadata)
  if artifact_cache_valid "$KERNEL_IMAGE" "$KERNEL_STAMP" "$stamp_metadata"; then
    echo "Using cached Linux ${LINUX_COMMIT} kernel image" >&2
    return
  fi

  mkdir -p "$CACHE_DIR" "$WORK_DIR"
  rm -f "$KERNEL_IMAGE" "$KERNEL_STAMP"
  checkout_commit "$LINUX_URL" "$LINUX_COMMIT" "$LINUX_REF" "$tree"
  make -C "$tree" ARCH=riscv CROSS_COMPILE="$CROSS_COMPILE" ch_defconfig

  for option in "${required_options[@]}"; do
    if [[ $("${tree}/scripts/config" --file "${tree}/.config" --state "$option") != y ]]; then
      echo "Cloud Hypervisor ch_defconfig does not enable ${option}" >&2
      return 1
    fi
  done

  make -C "$tree" ARCH=riscv CROSS_COMPILE="$CROSS_COMPILE" -j"$(nproc)" Image
  cp "${tree}/arch/riscv/boot/Image" "$KERNEL_IMAGE"
  write_artifact_stamp "$KERNEL_IMAGE" "$KERNEL_STAMP" "$stamp_metadata"
}

prepare_busybox() {
  local tarball="${WORK_DIR}/busybox-${BUSYBOX_VERSION}.tar.bz2"
  local tree="${WORK_DIR}/busybox-${BUSYBOX_VERSION}"
  local stamp_metadata

  stamp_metadata=$(busybox_stamp_metadata)
  if artifact_cache_valid "${BUSYBOX_INSTALL}/bin/busybox" "$BUSYBOX_STAMP" "$stamp_metadata" &&
    [[ -x "${BUSYBOX_INSTALL}/bin/sh" &&
      -x "${BUSYBOX_INSTALL}/bin/mount" &&
      -x "${BUSYBOX_INSTALL}/usr/bin/test" &&
      -x "${BUSYBOX_INSTALL}/sbin/poweroff" ]]; then
    echo "Using cached BusyBox ${BUSYBOX_VERSION} installation" >&2
    return
  fi

  mkdir -p "$CACHE_DIR" "$WORK_DIR"
  rm -rf "$BUSYBOX_INSTALL"
  rm -f "$BUSYBOX_STAMP"
  download_asset "$BUSYBOX_URL" "$tarball" "$BUSYBOX_SHA256"
  rm -rf "$tree"
  tar -xjf "$tarball" -C "$WORK_DIR"
  make -C "$tree" ARCH=riscv CROSS_COMPILE="$CROSS_COMPILE" defconfig

  sed -i 's/^# CONFIG_STATIC is not set$/CONFIG_STATIC=y/' "${tree}/.config"
  grep -q '^CONFIG_STATIC=y' "${tree}/.config" || echo 'CONFIG_STATIC=y' >>"${tree}/.config"

  # Linux 6.8 removed the CBQ UAPI constants used by BusyBox's unused tc
  # applet. Disable it so current cross toolchains can build the initramfs.
  sed -i 's/^CONFIG_TC=y$/# CONFIG_TC is not set/' "${tree}/.config"
  grep -q '^# CONFIG_TC is not set$' "${tree}/.config" || echo '# CONFIG_TC is not set' >>"${tree}/.config"

  make -C "$tree" ARCH=riscv CROSS_COMPILE="$CROSS_COMPILE" -j"$(nproc)"
  make -C "$tree" ARCH=riscv CROSS_COMPILE="$CROSS_COMPILE" install
  cp -a "${tree}/_install" "$BUSYBOX_INSTALL"
  write_artifact_stamp "${BUSYBOX_INSTALL}/bin/busybox" "$BUSYBOX_STAMP" "$stamp_metadata"
}

prepare_cloud_hypervisor() {
  local tree="${WORK_DIR}/cloud-hypervisor-${CLOUD_HYPERVISOR_COMMIT}"
  local built="${tree}/target/${RUST_TARGET}/release/cloud-hypervisor"
  local stamp_metadata

  stamp_metadata=$(cloud_hypervisor_stamp_metadata)
  if artifact_cache_valid "$CLOUD_HYPERVISOR_BIN" "$CLOUD_HYPERVISOR_STAMP" "$stamp_metadata" &&
    "${CROSS_COMPILE}readelf" -h "$CLOUD_HYPERVISOR_BIN" | grep -q 'Machine:.*RISC-V'; then
    echo "Using cached Cloud Hypervisor ${CLOUD_HYPERVISOR_VERSION}" >&2
    return
  fi

  mkdir -p "$CACHE_DIR" "$WORK_DIR"
  rm -f "$CLOUD_HYPERVISOR_BIN" "$CLOUD_HYPERVISOR_STAMP"
  checkout_commit \
    "$CLOUD_HYPERVISOR_URL" \
    "$CLOUD_HYPERVISOR_COMMIT" \
    "$CLOUD_HYPERVISOR_REF" \
    "$tree"
  CARGO_TARGET_RISCV64GC_UNKNOWN_LINUX_GNU_LINKER="${CROSS_COMPILE}gcc" \
    CC_riscv64gc_unknown_linux_gnu="${CROSS_COMPILE}gcc" \
    AR_riscv64gc_unknown_linux_gnu="${CROSS_COMPILE}ar" \
    cargo build \
      --manifest-path "${tree}/Cargo.toml" \
      --locked \
      --release \
      --target "$RUST_TARGET" \
      --package cloud-hypervisor

  cp "$built" "$CLOUD_HYPERVISOR_BIN"
  write_artifact_stamp "$CLOUD_HYPERVISOR_BIN" "$CLOUD_HYPERVISOR_STAMP" "$stamp_metadata"
}

build_guest_initramfs() {
  local rootfs="${WORK_DIR}/guest-rootfs"

  rm -rf "$rootfs"
  mkdir -p "$rootfs" "$WORK_DIR"
  cp -a "${BUSYBOX_INSTALL}/." "$rootfs/"
  mkdir -p "$rootfs/proc" "$rootfs/sys" "$rootfs/dev"

  cat >"$rootfs/init" <<EOF
#!/bin/sh
set -eu

/bin/mount -t proc none /proc
/bin/mount -t sysfs none /sys
/bin/mount -t devtmpfs none /dev

/usr/bin/test "\$(/bin/uname -m)" = riscv64
/usr/bin/test -r /proc/version
echo "${GUEST_MARKER} \$(/bin/uname -r)"
# Keep PID 1 alive until the outer test observes the marker and stops QEMU.
# Cloud Hypervisor v53 reports a guest shutdown event as a vCPU error even
# though the shutdown is otherwise successful, which would make a clean boot
# log look like a failure.
while :; do
  /bin/sleep 3600
done
EOF
  chmod +x "$rootfs/init"
  (cd "$rootfs" && find . -print0 | cpio --null --create --format=newc --quiet | gzip -9) >"$GUEST_INITRAMFS"
}

copy_dynamic_dependencies() {
  local binary=$1
  local rootfs=$2
  local elf dependency source interpreter search_dir
  local -a queue=("$binary")
  local -a dependencies
  declare -A copied=()

  interpreter=$("${CROSS_COMPILE}readelf" -l "$binary" |
    sed -n 's@.*Requesting program interpreter: \(.*\)]@\1@p')
  if [[ -z "$interpreter" || ! -f "${RISCV_SYSROOT}${interpreter}" ]]; then
    echo "Cannot resolve ELF interpreter for ${binary}: ${interpreter:-none}" >&2
    return 1
  fi
  mkdir -p "$(dirname "${rootfs}${interpreter}")"
  cp -L "${RISCV_SYSROOT}${interpreter}" "${rootfs}${interpreter}"

  while (( ${#queue[@]} != 0 )); do
    elf=${queue[0]}
    queue=("${queue[@]:1}")
    mapfile -t dependencies < <(
      "${CROSS_COMPILE}readelf" -d "$elf" |
        sed -n 's/.*Shared library: \[\([^]]*\)\].*/\1/p'
    )

    for dependency in "${dependencies[@]}"; do
      [[ -z "${copied[$dependency]:-}" ]] || continue
      source=""
      for search_dir in "${RISCV_SYSROOT}/lib" "${RISCV_SYSROOT}/usr/lib"; do
        [[ -d "$search_dir" ]] || continue
        source=$(find "$search_dir" -name "$dependency" -print -quit)
        [[ -z "$source" ]] || break
      done
      if [[ -z "$source" ]]; then
        echo "Cannot resolve ${dependency}, required by ${elf}" >&2
        return 1
      fi
      cp -L "$source" "${rootfs}/lib/${dependency}"
      copied[$dependency]=1
      queue+=("$source")
    done
  done
}

build_host_initramfs() {
  local rootfs="${WORK_DIR}/host-rootfs"

  rm -rf "$rootfs"
  mkdir -p "$rootfs" "$WORK_DIR"
  cp -a "${BUSYBOX_INSTALL}/." "$rootfs/"
  mkdir -p "$rootfs/proc" "$rootfs/sys" "$rootfs/dev" "$rootfs/run" "$rootfs/tmp"
  mkdir -p "$rootfs/lib" "$rootfs/usr/bin" "$rootfs/opt/guest"

  cp "$CLOUD_HYPERVISOR_BIN" "$rootfs/usr/bin/cloud-hypervisor"
  copy_dynamic_dependencies "$CLOUD_HYPERVISOR_BIN" "$rootfs"
  cp "$KERNEL_IMAGE" "$rootfs/opt/guest/Image"
  cp "$GUEST_INITRAMFS" "$rootfs/opt/guest/initramfs.cpio.gz"

  cat >"$rootfs/init" <<EOF
#!/bin/sh
set -eu

/bin/mount -t proc none /proc
/bin/mount -t sysfs none /sys
/bin/mount -t devtmpfs none /dev
exec </dev/console >/dev/console 2>&1

/usr/bin/test "\$(/bin/uname -m)" = riscv64
/usr/bin/test -c /dev/kvm
/usr/bin/test -r /dev/kvm
/usr/bin/test -w /dev/kvm
echo "${HOST_MARKER} \$(/bin/uname -r)"

/usr/bin/cloud-hypervisor \
  --kernel /opt/guest/Image \
  --initramfs /opt/guest/initramfs.cpio.gz \
  --cmdline "console=hvc0 rdinit=/init" \
  --cpus boot=1 \
  --memory size=512M \
  --console tty \
  --serial off \
  --seccomp false

/sbin/poweroff -f
EOF
  chmod +x "$rootfs/init"
  (cd "$rootfs" && find . -print0 | cpio --null --create --format=newc --quiet | gzip -9) >"$HOST_INITRAMFS"
}

require_command() {
  local command_name=$1
  if ! command -v "$command_name" >/dev/null; then
    echo "Missing required command: ${command_name}" >&2
    return 1
  fi
}

check_prerequisites() {
  local command_name
  local -a required_commands=(
    basename
    cargo
    cat
    chmod
    cp
    cpio
    curl
    dirname
    find
    git
    grep
    gzip
    make
    mkdir
    mktemp
    mv
    nproc
    rm
    rustc
    rustup
    sed
    sha256sum
    sleep
    tail
    tar
    "${CROSS_COMPILE}ar"
    "${CROSS_COMPILE}gcc"
    "${CROSS_COMPILE}readelf"
  )

  for command_name in "${required_commands[@]}"; do
    require_command "$command_name"
  done

  test -s "$RUSTSBI" || {
    echo "Missing $RUSTSBI; run 'cargo prototyper build --features hypervisor' first" >&2
    return 1
  }
  test -s "${RISCV_SYSROOT}/lib/libc.so.6" || {
    echo "Missing RISC-V sysroot libc under ${RISCV_SYSROOT}" >&2
    return 1
  }
  if ! rustup target list --installed | grep -Fxq "$RUST_TARGET"; then
    echo "Missing Rust target ${RUST_TARGET}; run 'rustup target add ${RUST_TARGET}'" >&2
    return 1
  fi

  SCRIPT_SHA256=$(sha256sum "${BASH_SOURCE[0]}")
  SCRIPT_SHA256=${SCRIPT_SHA256%% *}
  cargo --version
  rustc --version
  "${CROSS_COMPILE}gcc" --version | sed -n '1p'
}

stop_qemu() {
  if [[ -n "$QEMU_PID" ]] && kill -0 "$QEMU_PID" 2>/dev/null; then
    kill "$QEMU_PID" 2>/dev/null || true
    wait "$QEMU_PID" 2>/dev/null || true
  fi
}

start_qemu() {
  mkdir -p "$LOG_DIR"
  : >"$LOG_FILE"
  "$QEMU" \
    -machine virt,aia=aplic-imsic \
    -cpu rv64,h=true,smstateen=true \
    -smp 2 \
    -m 2G \
    -nographic \
    -no-reboot \
    -bios "$RUSTSBI" \
    -kernel "$KERNEL_IMAGE" \
    -initrd "$HOST_INITRAMFS" \
    -append "console=ttyS0 earlycon=sbi rdinit=/init" \
    >"$LOG_FILE" 2>&1 &
  QEMU_PID=$!
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
  echo "QEMU exited before the Cloud Hypervisor guest reached userspace (exit=${qemu_exit})" >&2
  tail -n 160 "$LOG_FILE" || true
}

wait_for_guest() {
  local elapsed
  for ((elapsed = 0; elapsed < BOOT_TIMEOUT_SECS; elapsed++)); do
    if grep -Fq "$HOST_MARKER" "$LOG_FILE" && grep -Fq "$GUEST_MARKER" "$LOG_FILE"; then
      return 0
    fi
    if boot_has_failed; then
      echo "Nested Linux boot failed:" >&2
      grep -E --max-count=5 "$BOOT_FAILURE_PATTERN" "$LOG_FILE" >&2 || true
      tail -n 160 "$LOG_FILE" || true
      return 1
    fi
    if ! kill -0 "$QEMU_PID" 2>/dev/null; then
      if grep -Fq "$HOST_MARKER" "$LOG_FILE" && grep -Fq "$GUEST_MARKER" "$LOG_FILE"; then
        return 0
      fi
      report_early_exit
      return 1
    fi
    sleep 1
  done

  if grep -Fq "$HOST_MARKER" "$LOG_FILE" && grep -Fq "$GUEST_MARKER" "$LOG_FILE"; then
    return 0
  fi
  echo "Cloud Hypervisor guest did not reach userspace within ${BOOT_TIMEOUT_SECS}s" >&2
  tail -n 160 "$LOG_FILE" || true
  return 1
}

main() {
  trap stop_qemu EXIT

  check_prerequisites
  prepare_qemu
  prepare_kernel
  prepare_busybox
  prepare_cloud_hypervisor
  build_guest_initramfs
  build_host_initramfs
  start_qemu
  wait_for_guest

  if ! grep -E "kvm \[[0-9]+\]: hypervisor extension available" "$LOG_FILE"; then
    echo "Note: the kernel did not emit its usual KVM H-extension diagnostic" >&2
  fi
  grep -F "$HOST_MARKER" "$LOG_FILE"
  grep -F "$GUEST_MARKER" "$LOG_FILE"
  echo "RustSBI booted a Cloud Hypervisor Linux guest to userspace successfully"
  echo "QEMU log: ${LOG_FILE}"
}

main "$@"
