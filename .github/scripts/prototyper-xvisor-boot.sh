#!/usr/bin/env bash
#
# Boot Xvisor on QEMU virt through RustSBI Prototyper: RustSBI hands over at
# 0x80200000, Xvisor relocates to 0x10000000 and runs a virt64 guest firmware
# that starts Linux to a BusyBox shell. Needs the firmware from
# `cargo prototyper build --features hypervisor`; everything else is built here,
# with the tools the workflow installs.
#
# Xvisor is a pinned commit verified at checkout; Linux and BusyBox are
# sha256-pinned tarballs whose digests are verified on every download. Only
# their build products are cached.
#
# Nobody types on the serial console, so the three nested consoles of the
# upstream flow (xvisor docs/riscv/riscv64-qemu.txt) are driven one prompt at a
# time: `XVisor#` kicks guest0 and binds its uart, `basic#` runs autoexec,
# `Please press Enter to activate this console.` needs Enter (without it
# BusyBox is booted but never prompts), and `/ #` runs the smoke command,
# `echo -n AA; echo BB`, whose typed halves the shell echoes apart so only the
# output can hold "AABB".
#
# Xvisor's own build-riscv-images.sh cannot build this guest on ubuntu-24.04 and
# has no `set -e`, so the guest images are built here instead.

set -euo pipefail

readonly XVISOR_VERSION="e700f46"
readonly XVISOR_COMMIT="e700f46cda1997f17e4f084de5e22e268ccae0aa"
readonly XVISOR_URL="https://github.com/xvisor/xvisor.git"

# The Linux tarball is fetched from the v6.x tree, unlike Xvisor's own
# tests/common/scripts/build-riscv-images.sh, which builds the URL against
# the v4.x tree and therefore 404s on its own default 6.1.1.
readonly LINUX_VERSION="6.1.1"
readonly LINUX_URL="https://cdn.kernel.org/pub/linux/kernel/v6.x/linux-6.1.1.tar.xz"
readonly LINUX_SHA256="a3e61377cf4435a9e2966b409a37a1056f6aaa59e561add9125a88e3c0971dfb"

readonly BUSYBOX_VERSION="1.33.1"
readonly BUSYBOX_URL="https://busybox.net/downloads/busybox-1.33.1.tar.bz2"
readonly BUSYBOX_SHA256="12cec6bd2b16d8a9446dd16130f2b92982f1819f6e1c5f5887b6db03f5660d28"

readonly CROSS_COMPILE="riscv64-linux-gnu-"

# The cross gcc's guest libc headers. gcc-riscv64-linux-gnu only *Recommends*
# libc6-dev-riscv64-cross, so `--no-install-recommends` leaves the compiler
# without a sysroot and it silently falls back to the host /usr/include.
readonly CROSS_SYSROOT_INC="/usr/${CROSS_COMPILE%-}/include"

# Prompts and gate strings, all taken from the serial log this flow produces.
readonly XVISOR_PROMPT='XVisor#'
readonly GUEST_FW_PROMPT='basic#'
readonly CONSOLE_ACTIVATE_PROMPT='activate this console'
readonly BASHY_PROMPT='/ #'
# Output-only: the shell echoes "AA" and "BB" apart, never "AABB".
readonly SMOKE_MARKER='AABB'

# The boot has gone wrong, and without this the job would sit out the whole
# timeout: a panic halts the machine while QEMU stays alive. The vfs/guest/
# vserial failures are Xvisor refusing a step of the upstream flow, and the
# rest are Linux giving up or RustSBI halting.
readonly BOOT_FAILURE_PATTERN='Failed to load fdt|Failed to open /images|Failed to find guest|Failed to find virtual serial port|Error: command|Kernel panic|not syncing|VFS: Cannot open root device|Attempted to kill init|No working init found|Please stand by while rebooting|System shutdown scheduled due to RustSBI panic|panic'

# Both build trees are made absolute up front: the builds run under
# `make -C <source>`, so a relative O= would be resolved against the source
# directory instead of the caller's.
CACHE_DIR="$(realpath -m "${XVISOR_CACHE_DIR:-.cache/xvisor}")"
readonly CACHE_DIR
WORK_DIR="$(realpath -m "${XVISOR_WORK_DIR:-.xvisor/work}")"
readonly WORK_DIR
readonly LOG_DIR="${QEMU_LOG_DIR:-qemu-logs}"
readonly LOG_FILE="${LOG_DIR}/prototyper-xvisor.log"
readonly BOOT_TIMEOUT_SECS="${XVISOR_BOOT_TIMEOUT_SECS:-300}"

readonly RUSTSBI="${XVISOR_RUSTSBI:-target/riscv64gc-unknown-none-elf/release/rustsbi-prototyper-dynamic.bin}"
readonly QEMU="${XVISOR_QEMU:-qemu-system-riscv64}"

# Only the expensive build outputs are cached; the source trees are rebuilt
# when missing, the way the NuttX script caches the ELF and not the checkout.
readonly ARTIFACT_CACHE="${CACHE_DIR}/${XVISOR_VERSION}-linux-${LINUX_VERSION}-busybox-${BUSYBOX_VERSION}"
readonly CACHED_VMM="${ARTIFACT_CACHE}/vmm.bin"
readonly CACHED_FIRMWARE="${ARTIFACT_CACHE}/firmware.bin"
readonly CACHED_DISK="${ARTIFACT_CACHE}/disk.ext2"

readonly XVISOR_SRC="${WORK_DIR}/xvisor"
readonly XVISOR_BUILD="${WORK_DIR}/build"
readonly TARBALL_DIR="${WORK_DIR}/tarball"
readonly DISK_DIR="${WORK_DIR}/disk"

QEMU_PID=""

# Check out an exact commit without keeping remote history, and refuse to
# build anything that is not the pinned commit.
git_checkout() {
  local url=$1
  local dir=$2
  local commit=$3
  local head

  rm -rf "$dir"
  mkdir -p "$dir"
  git init --quiet "$dir"
  git -C "$dir" remote add origin "$url"
  git -C "$dir" fetch --quiet --depth=1 origin "$commit"
  git -C "$dir" checkout --quiet --detach FETCH_HEAD
  head=$(git -C "$dir" rev-parse HEAD)
  if [[ "$head" != "$commit" ]]; then
    echo "Checkout mismatch for $url: expected ${commit}, got ${head}" >&2
    return 1
  fi
}

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

  if ! curl --fail --location \
    --connect-timeout 30 --max-time 1800 \
    --retry 5 --retry-all-errors --retry-max-time 1800 \
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

prepare_xvisor() {
  if [[ ! -d "${XVISOR_SRC}/.git" ]]; then
    git_checkout "$XVISOR_URL" "$XVISOR_SRC" "$XVISOR_COMMIT"
    patch_xvisor_march_order
  fi
}

# Xvisor's RISC-V objects.mk pins the H extension into the base ISA string
# (`march-y = rv64imh`) and then appends A, F, D, C and V behind it, so the
# compiler is handed -march=rv64imhafdcv. RISC-V requires the canonical
# extension order, and GCC >= 13 rejects the rest with "ISA string is not in
# canonical order", so Xvisor's RISC-V build fails outright on the Ubuntu
# 24.04 runner. Moving H behind V keeps the same instruction set and produces
# the accepted rv64imafdcvh.
#
# The rewrite is asserted rather than blind so a future Xvisor that fixes this
# upstream fails loudly instead of being silently mis-patched.
patch_xvisor_march_order() {
  local objects_mk="${XVISOR_SRC}/arch/riscv/cpu/generic/objects.mk"
  # The $(march-y)-style strings below are Xvisor's make variables that must
  # reach grep and sed literally, so they are deliberately single-quoted.
  # shellcheck disable=SC2016
  local patched_marker='arch-v-y)h$(march-zicsr-zifenci-y)'

  if grep -q '^march-y = rv64im$' "$objects_mk" &&
    grep -q "$patched_marker" "$objects_mk"; then
    echo "Xvisor already builds a canonical -march order" >&2
    return
  fi

  grep -q '^march-y = rv64imh$' "$objects_mk" ||
    { echo "unexpected march-y in $objects_mk" >&2; return 1; }
  grep -q '^march-y = rv32imh$' "$objects_mk" ||
    { echo "unexpected 32-bit march-y in $objects_mk" >&2; return 1; }

  # shellcheck disable=SC2016
  sed -i \
    -e 's/^march-y = rv64imh$/march-y = rv64im/' \
    -e 's/^march-y = rv32imh$/march-y = rv32im/' \
    -e 's|^march-nonld-isa-y = $(march-y)$(arch-a-y)fd$(arch-c-y)$(arch-v-y)$(march-zicsr-zifenci-y)$|march-nonld-isa-y = $(march-y)$(arch-a-y)fd$(arch-c-y)$(arch-v-y)h$(march-zicsr-zifenci-y)|' \
    -e 's|^march-ld-isa-y = $(march-y)$(arch-a-y)$(arch-c-y)$(arch-v-y)$|march-ld-isa-y = $(march-y)$(arch-a-y)$(arch-c-y)$(arch-v-y)h|' \
    "$objects_mk"

  if ! grep -q '^march-y = rv64im$' "$objects_mk" ||
    ! grep -q "$patched_marker" "$objects_mk"; then
    echo "failed to patch $objects_mk" >&2
    return 1
  fi
}

# Xvisor itself and the little virt64 guest firmware that stands in for a
# firmware payload.
build_hypervisor() {
  if [[ ! -s "$CACHED_VMM" || ! -s "$CACHED_FIRMWARE" ]]; then
    echo "Building Xvisor ${XVISOR_VERSION} and the virt64 guest firmware" >&2
    mkdir -p "$XVISOR_BUILD"
    make -C "$XVISOR_SRC" O="$XVISOR_BUILD" ARCH=riscv CROSS_COMPILE="$CROSS_COMPILE" \
      generic-64b-defconfig
    make -C "$XVISOR_SRC" O="$XVISOR_BUILD" ARCH=riscv CROSS_COMPILE="$CROSS_COMPILE" \
      -j"$(nproc)"
    make -C "${XVISOR_SRC}/tests/riscv/virt64/basic" O="$XVISOR_BUILD" \
      ARCH=riscv CROSS_COMPILE="$CROSS_COMPILE" -j"$(nproc)"

    test -s "${XVISOR_BUILD}/vmm.bin"
    test -s "${XVISOR_BUILD}/tests/riscv/virt64/basic/firmware.bin"
    mkdir -p "$ARTIFACT_CACHE"
    cp -f "${XVISOR_BUILD}/vmm.bin" "$CACHED_VMM"
    cp -f "${XVISOR_BUILD}/tests/riscv/virt64/basic/firmware.bin" "$CACHED_FIRMWARE"
  else
    echo "Using cached Xvisor ${XVISOR_VERSION}" >&2
  fi
}

# The guest kernel, configured the way Xvisor's virt64 test expects: its
# arch/riscv defconfig with tests/riscv/virt64/linux/linux_extra.config merged
# on top, which is what turns on the paravirtualized drivers the guest needs.
build_linux() {
  local source="${WORK_DIR}/linux-${LINUX_VERSION}"
  local output="${WORK_DIR}/linux-${LINUX_VERSION}-virt64"

  if [[ -s "${output}/arch/riscv/boot/Image" ]]; then
    echo "Using cached Linux ${LINUX_VERSION}" >&2
    return
  fi

  download_asset "$LINUX_URL" "${TARBALL_DIR}/linux-${LINUX_VERSION}.tar.xz" "$LINUX_SHA256"
  rm -rf "$source" "$output"
  tar -C "$WORK_DIR" -xf "${TARBALL_DIR}/linux-${LINUX_VERSION}.tar.xz"
  mkdir -p "$output"

  cp -f "${source}/arch/riscv/configs/defconfig" "${source}/arch/riscv/configs/tmp-virt64_defconfig"
  "${XVISOR_SRC}/tests/common/scripts/update-linux-defconfig.sh" \
    -p "${source}/arch/riscv/configs/tmp-virt64_defconfig" \
    -f "${XVISOR_SRC}/tests/riscv/virt64/linux/linux_extra.config"
  make -C "$source" O="$output" ARCH=riscv CROSS_COMPILE="$CROSS_COMPILE" tmp-virt64_defconfig
  make -C "$source" O="$output" ARCH=riscv CROSS_COMPILE="$CROSS_COMPILE" -j"$(nproc)" Image

  test -s "${output}/arch/riscv/boot/Image"
}

# BusyBox supplies the guest userspace. Two settings in Xvisor's own
# busybox defconfig have to change before it will build on a current Ubuntu:
#
#   CONFIG_CROSS_COMPILER_PREFIX is empty, and BusyBox ignores the
#   CROSS_COMPILE environment variable its build scripts export, so an empty
#   prefix builds it with the host compiler.
#
#   CONFIG_TC enables networking/tc.c, which cannot compile against the
#   current linux-libc-dev at all: struct tc_cbq_wrropt was dropped from
#   linux/pkt_sched.h, and it is absent from the riscv sysroot headers too.
build_busybox() {
  local source="${WORK_DIR}/busybox-${BUSYBOX_VERSION}"
  local install="${source}/_install"

  if [[ -s "${install}/bin/busybox" ]]; then
    echo "Using cached BusyBox ${BUSYBOX_VERSION}" >&2
    return
  fi

  download_asset "$BUSYBOX_URL" "${TARBALL_DIR}/busybox-${BUSYBOX_VERSION}.tar.bz2" "$BUSYBOX_SHA256"
  # busybox-<version>.tar.bz2 unpacks into busybox-<version>, which is
  # already $source, so there is nothing to move.
  rm -rf "$source"
  tar -C "$WORK_DIR" -xf "${TARBALL_DIR}/busybox-${BUSYBOX_VERSION}.tar.bz2"
  test -d "$source"

  cp -f "${XVISOR_SRC}/tests/common/busybox/busybox-${BUSYBOX_VERSION}_defconfig" "${source}/.config"
  sed -i \
    -e "s|^CONFIG_CROSS_COMPILER_PREFIX=.*|CONFIG_CROSS_COMPILER_PREFIX=\"${CROSS_COMPILE}\"|" \
    -e 's|^CONFIG_TC=y$|# CONFIG_TC is not set|' \
    "${source}/.config"
  # oldconfig can ask about symbols the defconfig does not mention, and must
  # not block on a prompt in CI. `yes '' | make oldconfig` would work, but
  # under `set -o pipefail` the yes(1) that gets SIGPIPE when make exits
  # makes the whole pipeline report 141 and abort the script, so feed it EOF
  # instead and let it take the defaults.
  make -C "$source" ARCH=riscv CROSS_COMPILE="$CROSS_COMPILE" oldconfig \
    </dev/null >/dev/null

  grep -q "^CONFIG_CROSS_COMPILER_PREFIX=\"${CROSS_COMPILE}\"$" "${source}/.config" ||
    { echo "BusyBox kept an empty cross compiler prefix" >&2; return 1; }
  grep -q '^# CONFIG_TC is not set$' "${source}/.config" ||
    { echo "BusyBox re-enabled CONFIG_TC, which cannot build here" >&2; return 1; }

  make -C "$source" ARCH=riscv CROSS_COMPILE="$CROSS_COMPILE" -j"$(nproc)"
  make -C "$source" ARCH=riscv CROSS_COMPILE="$CROSS_COMPILE" install >/dev/null

  test -s "${install}/bin/busybox" || {
    echo "BusyBox produced no binary; the guest rootfs would be empty" >&2
    return 1
  }
  file "${install}/bin/busybox" | grep -q RISC-V ||
    { echo "BusyBox is not a RISC-V binary" >&2; return 1; }
}

# Assemble the BusyBox RAM disk, with the same /init, fstab, rcS and motd
# Xvisor's virt64 test uses.
make_rootfs() {
  local install="${WORK_DIR}/busybox-${BUSYBOX_VERSION}/_install"
  local rootfs="${WORK_DIR}/rootfs.cpio"

  mkdir -p "${install}/etc/init.d" "${install}/dev" "${install}/proc" "${install}/sys"
  ln -sf /sbin/init "${install}/init"
  cp -f "${XVISOR_SRC}/tests/common/busybox/fstab" "${install}/etc/fstab"
  cp -f "${XVISOR_SRC}/tests/common/busybox/rcS" "${install}/etc/init.d/rcS"
  cp -f "${XVISOR_SRC}/tests/common/busybox/motd" "${install}/etc/motd"

  ( cd "$install" && find ./ | cpio -o -H newc > "$rootfs" )
  test -s "$rootfs"
}

# Stage the Xvisor guest disk. The guest device tree sits directly under
# images/riscv/, because that is where one_guest_virt64.xscript reads it from;
# everything the guest's NOR flash holds goes under images/riscv/virt64/.
make_disk() {
  local guest_images="${DISK_DIR}/images/riscv/virt64"
  local guest_dtb="${DISK_DIR}/images/riscv/virt64-guest.dtb"

  rm -rf "$DISK_DIR" "$CACHED_DISK"
  mkdir -p "$guest_images" "${DISK_DIR}/system" "${DISK_DIR}/tmp"

  cp -f "${XVISOR_SRC}/docs/banner/roman.txt" "${DISK_DIR}/system/banner.txt"
  cp -f "${XVISOR_SRC}/docs/logo/xvisor_logo_name.ppm" "${DISK_DIR}/system/logo.ppm"
  cp -f "${XVISOR_SRC}/tests/riscv/virt64/xscript/one_guest_virt64.xscript" \
    "${DISK_DIR}/boot.xscript"
  cp -f "${XVISOR_SRC}/tests/riscv/virt64/linux/nor_flash.list" "${guest_images}/nor_flash.list"
  cp -f "${XVISOR_SRC}/tests/riscv/virt64/linux/cmdlist" "${guest_images}/cmdlist"
  cp -f "$CACHED_FIRMWARE" "${guest_images}/firmware.bin"
  cp -f "${WORK_DIR}/linux-${LINUX_VERSION}-virt64/arch/riscv/boot/Image" "${guest_images}/Image"
  cp -f "${WORK_DIR}/rootfs.cpio" "${guest_images}/rootfs.img"

  dtc -q -I dts -O dtb -o "$guest_dtb" "${XVISOR_SRC}/tests/riscv/virt64/virt64-guest.dts"
  dtc -q -I dts -O dtb -o "${guest_images}/virt64.dtb" "${XVISOR_SRC}/tests/riscv/virt64/linux/virt64.dts"

  genext2fs -B 1024 -b 32768 -d "$DISK_DIR" "$CACHED_DISK" >/dev/null
  test -s "$CACHED_DISK"
  test -s "${guest_images}/rootfs.img"
}

check_prerequisites() {
  test -s "$RUSTSBI" || {
    echo "Missing $RUSTSBI; run 'cargo prototyper build --features hypervisor' first" >&2
    return 1
  }
  "$QEMU" --version
  command -v dtc genext2fs cpio git curl file python3 "${CROSS_COMPILE}gcc" >/dev/null
  test -f "${CROSS_SYSROOT_INC}/limits.h" || {
    echo "Missing ${CROSS_SYSROOT_INC}/limits.h; the cross toolchain has no sysroot." >&2
    echo "Install libc6-dev-riscv64-cross (Debian/Ubuntu) alongside ${CROSS_COMPILE}gcc." >&2
    return 1
  }
}

stop_qemu() {
  if [[ -n "$QEMU_PID" ]] && kill -0 "$QEMU_PID" 2>/dev/null; then
    kill "$QEMU_PID" 2>/dev/null || true
    wait "$QEMU_PID" 2>/dev/null || true
  fi
}

# Wait for a prompt before answering it. Sending earlier could be dropped
# before the console that should receive it is even open.
wait_for() {
  local pattern=$1
  local limit=$2
  local i

  for ((i = 0; i < limit; i++)); do
    grep -aFq "$pattern" "$LOG_FILE" 2>/dev/null && return 0
    sleep 1
  done
  echo "[feed] never saw '${pattern}'" >&2
  return 1
}

# Start the guest. The guest firmware is loaded into the guest's NOR flash by
# the xscript, so kicking the guest is all that is left to do; the vserial
# bind is what routes the guest's console onto this serial port, and without
# it the guest is silent no matter how long it runs.
kick_guest() {
  wait_for "$XVISOR_PROMPT" 90 || return 1
  printf 'guest kick guest0\r'
  sleep 3
  printf 'vserial bind guest0/uart0\r'
  sleep 3
}

# autoexec replays the guest NOR cmdlist, which copies the kernel and the
# rootfs into guest RAM and starts them.
boot_guest_linux() {
  wait_for "$GUEST_FW_PROMPT" 90 || return 1
  printf 'autoexec\r'
}

# The BusyBox console is inactive until something is typed, so without this
# Enter the guest is fully booted but never prints a prompt.
activate_console() {
  wait_for "$CONSOLE_ACTIVATE_PROMPT" 180 || return 1
  printf '\r'
  sleep 5
}

run_smoke_test() {
  wait_for "$BASHY_PROMPT" 60 || return 1
  sleep 3
  printf 'echo -n AA; echo BB\r'
}

feed_console() {
  kick_guest          || return
  boot_guest_linux    || return
  activate_console    || return
  run_smoke_test      || return
}

userspace_is_ready() {
  grep -aFq "$SMOKE_MARKER" "$LOG_FILE"
}

boot_has_failed() {
  grep -aEq "$BOOT_FAILURE_PATTERN" "$LOG_FILE"
}

report_boot_failure() {
  echo "Xvisor failed to boot:" >&2
  # The serial log carries terminal control bytes; -a keeps grep printing the
  # matching lines instead of "binary file matches".
  grep -a -E --max-count=5 "$BOOT_FAILURE_PATTERN" "$LOG_FILE" >&2 || true
  tail -n 120 "$LOG_FILE" >&2 || true
}

report_early_exit() {
  local qemu_exit
  set +e
  wait "$QEMU_PID"
  qemu_exit=$?
  set -e

  echo "QEMU exited before the guest reached userspace (exit=${qemu_exit})" >&2
  tail -n 120 "$LOG_FILE" >&2 || true
}

start_qemu() {
  mkdir -p "$LOG_DIR"
  # Truncate any stale log from a previous run before QEMU starts, so the
  # wait loop cannot mistake an old success marker for this boot's.
  : >"$LOG_FILE"
  feed_console | "$QEMU" \
    -machine virt \
    -m 2G \
    -nographic \
    -no-reboot \
    -bios "$RUSTSBI" \
    -kernel "$CACHED_VMM" \
    -initrd "$CACHED_DISK" \
    -append 'vmm.bootcmd="vfs mount initrd /;vfs run /boot.xscript;vfs cat /system/banner.txt"' \
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

  echo "The Xvisor Linux guest did not reach userspace within ${BOOT_TIMEOUT_SECS}s" >&2
  tail -n 120 "$LOG_FILE" >&2 || true
  return 1
}

main() {
  trap stop_qemu EXIT

  check_prerequisites
  # Only the build products are cached, so a warm cache can skip the whole build
  # path; the disk is baked with the guest DTB and QEMU reads it from there.
  if [[ -s "$CACHED_VMM" && -s "$CACHED_FIRMWARE" && -s "$CACHED_DISK" ]]; then
    echo "Using cached Xvisor ${XVISOR_VERSION} and guest disk" >&2
  else
    prepare_xvisor
    build_hypervisor
    build_linux
    build_busybox
    make_rootfs
    make_disk
  fi

  start_qemu
  wait_for_userspace
  stop_qemu

  echo "RustSBI booted Xvisor ${XVISOR_VERSION} to a Linux BusyBox shell successfully"
  echo "QEMU log: ${LOG_FILE}"
}

main "$@"
