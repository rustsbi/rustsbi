#!/usr/bin/env bash
#
# Boot a minimal Linux userspace through RustSBI Prototyper in QEMU.
#
# RustSBI's dynamic firmware follows the fw_dynamic handoff convention, so QEMU
# starts the kernel directly: `-bios` points at the firmware and `-kernel` at
# the flattened kernel image, with QEMU handing the kernel entry point to the
# firmware through the dynamic info structure. No intermediate bootloader takes
# part, which keeps the boot path short and the CI job fast.
#
# A U-Boot-based boot path is selected with `$0 u-boot`: the SPL embeds
# RustSBI Prototyper as its OpenSBI payload, so the chain is QEMU -> u-boot-spl
# -> RustSBI -> u-boot.itb -> Linux. U-Boot loads the kernel off an ext4
# partition with `ext4load` and starts it with `booti`, matching the repository
# guide firmware/docs/booting-linux-kernel-in-qemu-using-uboot-and-rustsbi.md.
#
# An EDK II-based boot path is selected with `$0 edk2`: QEMU starts RustSBI's
# dynamic firmware, which launches the pinned EDK II RISC-V QEMU payload. EDK
# II then loads Linux through the EFI stub. The initramfs verifies that
# /sys/firmware/efi exists before it prints the success marker.
#
# The sbi boot also tests SBI HSM CPU hotplug (issue #262): before the success
# marker, the initramfs cycles every secondary CPU through sysfs offline/online
# around SBI HSM suspend entries.
#
# Linux and BusyBox are built from checksum-pinned release archives. Only the
# build *products* are kept under CACHE_DIR, so the workflow restores a small
# cache and skips compilation entirely on a hit; the archives themselves are
# verified on every download. RustSBI is rebuilt from the commit under test and
# is never cached.
#
# The U-Boot build is never cached: the SPL embeds the RustSBI firmware, so it
# must be rebuilt from the commit under test every run.
#
# Requires: `cargo prototyper build` to have produced the firmware, plus
# qemu-system-riscv64, riscv64-linux-gnu-gcc, cpio, xz and a host toolchain.
# The u-boot path additionally needs swig, python3-dev, parted, e2fsprogs and
# qemu-utils. The edk2 path additionally needs acpica-tools, nasm and uuid-dev.
# The sbi path additionally needs dtc to patch idle states into the device tree.

set -euo pipefail

# shellcheck source=.github/scripts/ci-uboot.sh
source "$(dirname "${BASH_SOURCE[0]}")/ci-uboot.sh"
# shellcheck source=.github/scripts/ci-edk2.sh
source "$(dirname "${BASH_SOURCE[0]}")/ci-edk2.sh"

if (( $# > 1 )); then
  echo "Usage: $0 [sbi|u-boot|edk2]" >&2
  exit 2
fi

readonly BOOT_MODE="${1:-sbi}"
case "$BOOT_MODE" in
  sbi | u-boot | edk2) ;;
  *)
    echo "Unknown boot mode: ${BOOT_MODE}" >&2
    echo "Usage: $0 [sbi|u-boot|edk2]" >&2
    exit 2
    ;;
esac

readonly KERNEL_VERSION="6.12.110"
readonly KERNEL_URL="https://cdn.kernel.org/pub/linux/kernel/v6.x/linux-${KERNEL_VERSION}.tar.xz"
readonly KERNEL_SHA256="8cee19e1839bb6ff4d5254d761933ae6ab670492d5ed030e09a80538320d5c4c"

readonly BUSYBOX_VERSION="1.36.1"
readonly BUSYBOX_URL="https://busybox.net/downloads/busybox-${BUSYBOX_VERSION}.tar.bz2"
readonly BUSYBOX_SHA256="b8cc24c9574d809e7279c3be349795c5d5ceb6fdf19ca709f80cde50e47de314"

readonly UB_VERSION="2024.04"

readonly CROSS_COMPILE="riscv64-linux-gnu-"
readonly SMOKE_MARKER="RUSTSBI-SMOKE-OK"

# Two CPUs give the sbi boot a secondary CPU to hotplug.
readonly SBI_SMP=2

# Text that means the boot already went wrong. A panic halts the machine while
# QEMU stays alive, so without this the job would sit out the whole timeout and
# then report only that the marker never appeared.
readonly BOOT_FAILURE_PATTERN="Kernel panic|not syncing|Attempted to kill init"

readonly CACHE_DIR="${MINIMAL_LINUX_CACHE_DIR:-.cache/minimal-linux}"
readonly WORK_DIR="${MINIMAL_LINUX_WORK_DIR:-.minimal-linux/work}"
readonly LOG_DIR="${QEMU_LOG_DIR:-qemu-logs}"
readonly LOG_FILE="${LOG_DIR}/prototyper-minimal-linux-${BOOT_MODE}.log"

# The sbi path boots the dynamic firmware directly; the u-boot path embeds the
# flattened binary in the SPL; and the edk2 path boots the dynamic binary so
# EDK II can receive the next-stage handoff. Bootloader paths get more time.
if [[ "$BOOT_MODE" = u-boot ]]; then
  readonly RUSTSBI="${MINIMAL_LINUX_RUSTSBI_BIN:-target/riscv64gc-unknown-none-elf/release/rustsbi-prototyper.bin}"
  readonly BOOT_TIMEOUT_SECS="${MINIMAL_LINUX_BOOT_TIMEOUT_SECS:-240}"
elif [[ "$BOOT_MODE" = edk2 ]]; then
  readonly RUSTSBI="${MINIMAL_LINUX_RUSTSBI_DYNAMIC_BIN:-target/riscv64gc-unknown-none-elf/release/rustsbi-prototyper-dynamic.bin}"
  readonly BOOT_TIMEOUT_SECS="${MINIMAL_LINUX_BOOT_TIMEOUT_SECS:-240}"
else
  readonly RUSTSBI="${MINIMAL_LINUX_RUSTSBI:-target/riscv64gc-unknown-none-elf/release/rustsbi-prototyper-dynamic.elf}"
  readonly BOOT_TIMEOUT_SECS="${MINIMAL_LINUX_BOOT_TIMEOUT_SECS:-180}"
fi

readonly KERNEL_IMAGE="${CACHE_DIR}/linux-${KERNEL_VERSION}-Image"
readonly BUSYBOX_INSTALL="${CACHE_DIR}/busybox-${BUSYBOX_VERSION}-install"
readonly INITRAMFS="${WORK_DIR}/initramfs.cpio.gz"
readonly DISK_IMAGE="${WORK_DIR}/linux-rootfs.img"
readonly IDLE_DTB="${WORK_DIR}/virt-idle-smp${SBI_SMP}.dtb"
# Populated by build_uboot; kept as mutable globals because the tree lives
# under WORK_DIR; only the FIT is repacked every run.
UB_SPL=""
UB_ITB=""

readonly DOWNLOAD_CONNECT_TIMEOUT_SECS="${MINIMAL_LINUX_DOWNLOAD_CONNECT_TIMEOUT_SECS:-30}"
readonly DOWNLOAD_TIMEOUT_SECS="${MINIMAL_LINUX_DOWNLOAD_TIMEOUT_SECS:-900}"

QEMU_PID=""
# The loop device and mount point that make_rootfs_image opens, tracked so the
# EXIT cleanup can release them if a setup step fails under `set -e`.
LOOP_DEVICE=""
ROOTFS_MOUNT=""

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

# `make Image` stops short of modules and device trees, neither of which this
# boot test loads.
prepare_kernel() {
  local tarball="${WORK_DIR}/linux-${KERNEL_VERSION}.tar.xz"
  local tree="${WORK_DIR}/linux-${KERNEL_VERSION}"
  local option
  local -a required_options

  if [[ -s "$KERNEL_IMAGE" ]]; then
    echo "Using cached Linux ${KERNEL_VERSION} kernel image" >&2
    return
  fi

  mkdir -p "$WORK_DIR" "$CACHE_DIR"
  download_asset "$KERNEL_URL" "$tarball" "$KERNEL_SHA256"

  rm -rf "$tree"
  tar -xJf "$tarball" -C "$WORK_DIR"
  make -C "$tree" ARCH=riscv CROSS_COMPILE="$CROSS_COMPILE" defconfig

  # The riscv defconfig is expected to carry everything this boot needs, so
  # check it rather than trust it: if a defconfig revision drops one of these,
  # the job should say which option went missing instead of booting into an
  # unexplained silence.
  # The sbi path needs an initramfs; the u-boot path boots off an ext4
  # partition on a virtio-mmio block device (the `-device virtio-blk-device`
  # QEMU attaches below) and a GPT table (EFI_PARTITION).
  if [[ "$BOOT_MODE" = u-boot ]]; then
    required_options=(EXT4_FS VIRTIO_BLK VIRTIO_MMIO EFI_PARTITION SERIAL_8250_CONSOLE DEVTMPFS)
  else
    required_options=(BLK_DEV_INITRD SERIAL_8250_CONSOLE DEVTMPFS)
    if [[ "$BOOT_MODE" = edk2 ]]; then
      required_options+=(EFI EFI_STUB)
    fi
  fi
  for option in "${required_options[@]}"; do
    if [[ $("${tree}/scripts/config" --file "${tree}/.config" --state "$option") != y ]]; then
      echo "riscv defconfig no longer enables ${option}" >&2
      return 1
    fi
  done

  make -C "$tree" ARCH=riscv CROSS_COMPILE="$CROSS_COMPILE" -j"$(nproc)" Image

  cp "${tree}/arch/riscv/boot/Image" "$KERNEL_IMAGE"
}

# BusyBox is linked statically because the initramfs carries no shared
# libraries. `make install` lays out the applet symlinks under _install, which
# is what gets cached.
prepare_busybox() {
  local tarball="${WORK_DIR}/busybox-${BUSYBOX_VERSION}.tar.bz2"
  local tree="${WORK_DIR}/busybox-${BUSYBOX_VERSION}"

  if [[ -x "${BUSYBOX_INSTALL}/bin/busybox" ]]; then
    echo "Using cached BusyBox ${BUSYBOX_VERSION} installation" >&2
    return
  fi

  mkdir -p "$WORK_DIR" "$CACHE_DIR"
  download_asset "$BUSYBOX_URL" "$tarball" "$BUSYBOX_SHA256"

  rm -rf "$tree"
  tar -xjf "$tarball" -C "$WORK_DIR"
  make -C "$tree" ARCH=riscv CROSS_COMPILE="$CROSS_COMPILE" defconfig

  # Settings -> Build Options -> Build static binary (no shared libs). defconfig
  # leaves CONFIG_STATIC unset; the build reads .config directly, so flipping
  # the line is enough and no oldconfig pass is needed.
  sed -i 's/^# CONFIG_STATIC is not set$/CONFIG_STATIC=y/' "${tree}/.config"
  grep -q '^CONFIG_STATIC=y' "${tree}/.config" || echo 'CONFIG_STATIC=y' >>"${tree}/.config"

  # `tc` still uses the CBQ scheduler constants, which Linux 6.8 dropped from
  # its UAPI headers, so the applet no longer compiles against a current
  # toolchain (busybox 1.37.0 is affected too). An initramfs smoke test has no
  # use for traffic control, so drop the applet rather than pin old headers.
  sed -i 's/^CONFIG_TC=y$/# CONFIG_TC is not set/' "${tree}/.config"
  grep -q '^# CONFIG_TC is not set$' "${tree}/.config" || echo '# CONFIG_TC is not set' >>"${tree}/.config"

  make -C "$tree" ARCH=riscv CROSS_COMPILE="$CROSS_COMPILE" -j"$(nproc)"
  make -C "$tree" ARCH=riscv CROSS_COMPILE="$CROSS_COMPILE" install

  rm -rf "$BUSYBOX_INSTALL"
  cp -a "${tree}/_install" "$BUSYBOX_INSTALL"
}

# CPU-hotplug test item for the sbi boot (issue #262). A separate script keeps
# /init a plain assertion list; a non-zero exit makes /init fail under `set -e`,
# which the kernel reports as "Attempted to kill init".
write_hotplug_test() {
  cat >"$1/hotplug.sh" <<'EOF'
#!/bin/sh
CPUDIR=/sys/devices/system/cpu
TAG=HOTPLUG
ROUNDS=6
XROUNDS=3
SETTLE=0.5
NPASS=0
NFAIL=0

pass() { echo "$TAG PASS: $1"; NPASS=$((NPASS + 1)); }
fail() { echo "$TAG FAIL: $1"; NFAIL=$((NFAIL + 1)); }

NP=$(nproc)
dmesg | grep -q 'SBI HSM extension' || fail 'SBI HSM extension not detected'

online_count() {
  n=1
  for c in "$CPUDIR"/cpu[0-9]*; do
    case "$c" in */cpu0) continue ;; esac
    [ "$(cat "$c/online" 2>/dev/null)" = "1" ] && n=$((n + 1))
  done
  echo "$n"
}

# The issue #262 reproduction: each offline makes the CPU call sbi_hart_stop(),
# each online calls sbi_hart_start().
echo "$TAG: per-CPU offline/online"
i=1
while [ "$i" -lt "$NP" ]; do
  echo 0 >"$CPUDIR/cpu$i/online" 2>/dev/null || fail "cpu$i offline"
  sleep "$SETTLE"
  [ "$(cat "$CPUDIR/cpu$i/online")" = "0" ] || fail "cpu$i offline state"
  echo 1 >"$CPUDIR/cpu$i/online" 2>/dev/null || fail "cpu$i online"
  sleep "$SETTLE"
  [ "$(cat "$CPUDIR/cpu$i/online")" = "1" ] || fail "cpu$i online state"
  i=$((i + 1))
done
[ "$NFAIL" = "0" ] && pass 'per-CPU offline/online'

echo "$TAG: $ROUNDS rounds of all-secondary offline/online"
r=1
while [ "$r" -le "$ROUNDS" ]; do
  i=1
  while [ "$i" -lt "$NP" ]; do
    echo 0 >"$CPUDIR/cpu$i/online" 2>/dev/null || fail "round $r: cpu$i offline"
    i=$((i + 1))
  done
  sleep "$SETTLE"
  n=$(online_count)
  [ "$n" = "1" ] || fail "round $r: expected 1 online CPU, have $n"
  i=1
  while [ "$i" -lt "$NP" ]; do
    echo 1 >"$CPUDIR/cpu$i/online" 2>/dev/null || fail "round $r: cpu$i online"
    i=$((i + 1))
  done
  sleep "$SETTLE"
  n=$(online_count)
  [ "$n" = "$NP" ] || fail "round $r: expected $NP online CPUs, have $n"
  r=$((r + 1))
done
[ "$NFAIL" = "0" ] && pass "$ROUNDS rounds all-secondary offline/online"

# A restarted CPU must compute: pin a counting loop to it and require its
# /proc/stat user time to advance.
echo "$TAG: compute liveness on each restarted CPU"
i=1
while [ "$i" -lt "$NP" ]; do
  before=$(awk -v c="cpu$i" '$1 == c {print $2 + $4}' /proc/stat)
  taskset -c "$i" sh -c 'k=0; while [ "$k" -lt 20000 ]; do k=$((k + 1)); done'
  after=$(awk -v c="cpu$i" '$1 == c {print $2 + $4}' /proc/stat)
  [ -n "$before" ] && [ "$after" -gt "$before" ] && pass "cpu$i executes work" \
    || fail "cpu$i user time $before -> $after"
  i=$((i + 1))
done

# Cross hotplug with deep idle: background sleepers keep every CPU entering
# and leaving its deepest idle state, which the patched DTB routes through
# SBI HSM hart_suspend.
echo "$TAG: hotplug during deep-idle activity"
j=0
SLEEPER_PIDS=""
while [ "$j" -lt $((NP * 3)) ]; do
  (while :; do sleep 0.05; done) &
  SLEEPER_PIDS="$SLEEPER_PIDS $!"
  j=$((j + 1))
done
r=1
while [ "$r" -le "$XROUNDS" ]; do
  i=1
  while [ "$i" -lt "$NP" ]; do
    echo 0 >"$CPUDIR/cpu$i/online" 2>/dev/null || fail "idle round $r: cpu$i offline"
    i=$((i + 1))
  done
  sleep "$SETTLE"
  i=1
  while [ "$i" -lt "$NP" ]; do
    echo 1 >"$CPUDIR/cpu$i/online" 2>/dev/null || fail "idle round $r: cpu$i online"
    i=$((i + 1))
  done
  sleep "$SETTLE"
  n=$(online_count)
  [ "$n" = "$NP" ] || fail "idle round $r: expected $NP online CPUs, have $n"
  r=$((r + 1))
done
# Kill sleepers by PID: killall sh would kill this script as well.
for p in $SLEEPER_PIDS; do
  kill "$p" 2>/dev/null
done
killall sleep 2>/dev/null
wait 2>/dev/null
[ "$NFAIL" = "0" ] && pass "$XROUNDS hotplug rounds during deep-idle activity"

# The kernel log must stay clean of the issue #262 signatures.
SIG='Unable to stop the cpu|arch_cpu_idle_dead|kernel BUG|Kernel panic|Oops:'
if dmesg | grep -Eq "$SIG"; then
  dmesg | grep -E "$SIG" | head -3 | sed "s/^/$TAG FAIL: /"
  NFAIL=$((NFAIL + 1))
else
  pass 'no BUG/panic/Oops signatures in kernel log'
fi

echo "$TAG: summary: pass=$NPASS fail=$NFAIL nproc=$(nproc)"
[ "$NFAIL" = "0" ] && [ "$(nproc)" = "$NP" ]
EOF
  chmod +x "$1/hotplug.sh"
}

# An initramfs is unpacked into the initial root filesystem, so the kernel runs
# /init directly. The marker proves a real userspace started; powering off
# afterwards lets the job finish without waiting out the boot timeout. The
# initramfs is rebuilt every run so edits to /init cannot go stale in a cache.
build_initramfs() {
  local rootfs="${WORK_DIR}/rootfs"
  local efi_assertion=""
  local hotplug_call=""

  if [[ "$BOOT_MODE" = edk2 ]]; then
    # Prove that the kernel entered through the EFI stub instead of merely
    # reaching the same userspace by another boot path.
    efi_assertion="/usr/bin/test -d /sys/firmware/efi"
  elif [[ "$BOOT_MODE" = sbi ]]; then
    hotplug_call='/bin/sh /hotplug.sh'
  fi

  rm -rf "$rootfs"
  mkdir -p "$rootfs" "$WORK_DIR"
  cp -a "${BUSYBOX_INSTALL}/." "$rootfs/"
  mkdir -p "$rootfs/proc" "$rootfs/sys" "$rootfs/dev"
  if [[ -n "$hotplug_call" ]]; then
    write_hotplug_test "$rootfs"
  fi

  # The assertions make the marker mean something specific: a riscv64 kernel
  # reached userspace and mounted a working /proc, not merely that some shell
  # ran. `set -eu` turns a failed assertion into a non-zero exit, which the
  # kernel reports as "Attempted to kill init" and the wait loop recognises as
  # a boot failure.
  cat >"$rootfs/init" <<EOF
#!/bin/sh
set -eu

/bin/mount -t proc none /proc
/bin/mount -t sysfs none /sys
/bin/mount -t devtmpfs none /dev

/usr/bin/test "\$(/bin/uname -m)" = riscv64
/usr/bin/test -r /proc/version
${efi_assertion}
${hotplug_call}

echo "${SMOKE_MARKER} \$(/bin/uname -r)"
/sbin/poweroff -f
EOF
  chmod +x "$rootfs/init"

  (cd "$rootfs" && find . -print0 | cpio --null --create --format=newc --quiet | gzip -9) >"$INITRAMFS"
}

# Patch an SBI non-retentive idle state into QEMU's device tree so deep idle
# enters through SBI HSM hart_suspend instead of plain WFI, crossing the
# suspend path with CPU hotplug in the same firmware hart state machine. The
# dump command must match start_qemu_sbi on SMP count and memory size, since
# the dumped tree describes exactly that machine.
prepare_idle_dtb() {
  local dts="${IDLE_DTB%.dtb}.dts"
  local tagged

  if [[ -s "$IDLE_DTB" ]]; then
    return
  fi

  qemu-system-riscv64 -machine virt,dumpdtb="${dts}.dtb" -smp "$SBI_SMP" -m 512M -display none >/dev/null 2>&1
  dtc -q -I dtb -O dts -o "$dts" "${dts}.dtb"

  # The idle-states node must land after the /cpus properties and before its
  # first subnode, which is the order dtc requires.
  awk '
    /^\t\tcpu@[0-9a-f]+ \{$/ && !done {
      print "\t\tidle-states {"
      print "\t\t\tentry-method = \"riscv,sbi-standard-extension\";"
      print "\t\t\tNONRET: nonret-default {"
      print "\t\t\t\tcompatible = \"riscv,idle-state\";"
      print "\t\t\t\triscv,sbi-suspend-param = <0x80000000>;"
      print "\t\t\t\tlocal-timer-stop;"
      print "\t\t\t\tentry-latency-us = <0x3e8>;"
      print "\t\t\t\texit-latency-us = <0x2710>;"
      print "\t\t\t\tmin-residency-us = <0x9c40>;"
      print "\t\t\t};"
      print "\t\t};"
      print ""
      done = 1
    }
    /^\t\t\tdevice_type = "cpu";$/ {
      print
      print "\t\t\tcpu-idle-states = <&NONRET>;"
      next
    }
    { print }
  ' "$dts" >"${dts}.patched"

  tagged=$(grep -c 'cpu-idle-states = <&NONRET>;' "${dts}.patched")
  if [[ ! -s "$dts" ]] || [[ "$tagged" != "$SBI_SMP" ]]; then
    echo "Failed to patch idle states into the QEMU device tree (tagged $tagged of $SBI_SMP cpu nodes)" >&2
    return 1
  fi

  dtc -q -I dts -O dtb -o "$IDLE_DTB" "${dts}.patched"
}

# Build a 1GiB GPT-partitioned disk image whose first (ext4) partition holds
# the kernel image and the BusyBox root filesystem. The image is rebuilt every
# run so edits to rcS cannot go stale in a cache.
make_rootfs_image() {
  local rootfs="${WORK_DIR}/rootfs"
  local loop part start end size

  rm -rf "$rootfs" "$DISK_IMAGE"
  mkdir -p "$WORK_DIR"

  qemu-img create -q "$DISK_IMAGE" 1g
  parted -s "$DISK_IMAGE" mklabel gpt
  parted -s "$DISK_IMAGE" mkpart primary ext4 1MiB 100%
  parted -s "$DISK_IMAGE" set 1 boot on

  # Read the exact byte range of partition 1. The sizelimit must match the
  # partition exactly: if mkfs runs over a loop device that reaches the end of
  # the file, it overwrites the backup GPT header and the kernel later rejects
  # the filesystem as having bad geometry. Using offset+sizelimit also avoids
  # needing /dev/loopNpM partition nodes, which udev-less containers lack.
  part=$(parted -s "$DISK_IMAGE" unit B print | awk '/^ 1 /{print $2, $3}')
  start=$(echo "$part" | awk '{gsub(/B/,"",$1); print $1}')
  end=$(echo "$part" | awk '{gsub(/B/,"",$2); print $2}')
  size=$((end - start + 1))

  loop=$(priv losetup --find --show --offset "$start" --sizelimit "$size" "$DISK_IMAGE")
  LOOP_DEVICE="$loop"
  priv mkfs.ext4 -q "$loop"

  mkdir -p "$rootfs"
  priv mount "$loop" "$rootfs"
  ROOTFS_MOUNT="$rootfs"

  # After `sudo mount` the mount point is owned by root, so a non-root CI
  # runner cannot write to it. Hand it to the current user for the copies
  # below (a no-op when already running as root in a local container).
  priv chown "$(id -u):$(id -g)" "$rootfs"

  cp "$KERNEL_IMAGE" "$rootfs/Image"
  cp -a "${BUSYBOX_INSTALL}/." "$rootfs/"
  mkdir -p "$rootfs/proc" "$rootfs/sys" "$rootfs/dev" "$rootfs/etc/init.d"

  # busybox init runs /etc/init.d/rcS. The assertions make the marker mean
  # something specific: a riscv64 kernel reached userspace and mounted a
  # working /proc, not merely that some shell ran.
  cat >"$rootfs/etc/init.d/rcS" <<'EOF'
#!/bin/sh
mount -t proc none /proc
mount -t sysfs none /sys
/sbin/mdev -s
set -e
[ "$(uname -m)" = riscv64 ]
[ -r /proc/version ]
echo "RUSTSBI-SMOKE-OK"
EOF
  chmod +x "$rootfs/etc/init.d/rcS"

  priv umount "$rootfs"
  ROOTFS_MOUNT=""
  priv losetup -d "$loop"
  LOOP_DEVICE=""
  rmdir "$rootfs"
}

# Build U-Boot with the RustSBI firmware embedded as the OpenSBI payload. The
# compiled products are shared; the FIT is repacked with the current RustSBI.
build_uboot() {
  # Keep compiled U-Boot independent of the current RustSBI FIT payload.
  local tree="${WORK_DIR}/u-boot"
  ci_uboot_build "$UB_VERSION" spl minimal-linux-spl "$tree"
  ci_uboot_fit "$tree" "$RUSTSBI"
  UB_SPL="$tree/spl/u-boot-spl"
  UB_ITB="$tree/u-boot.itb"
}

# Run a command with elevated privileges when running as a non-root user (CI
# runners) while staying a plain call inside a root container (local Docker).
priv() {
  if [[ $EUID -eq 0 ]]; then
    "$@"
  else
    sudo "$@"
  fi
}

check_prerequisites() {
  test -s "$RUSTSBI" || {
    echo "Missing $RUSTSBI; run 'cargo prototyper build' first" >&2
    return 1
  }
  if [[ "$BOOT_MODE" = sbi ]] && ! command -v dtc >/dev/null; then
    echo "sbi mode requires dtc (device-tree-compiler) to patch idle states" >&2
    return 1
  fi
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
  # Release any loop device or mount left behind by a failed setup step, so a
  # local rerun does not leak them and exhaust loop devices.
  if [[ -n "$ROOTFS_MOUNT" ]]; then
    priv umount "$ROOTFS_MOUNT" 2>/dev/null || true
  fi
  if [[ -n "$LOOP_DEVICE" ]]; then
    priv losetup -d "$LOOP_DEVICE" 2>/dev/null || true
  fi
}

start_qemu_sbi() {
  mkdir -p "$LOG_DIR"
  # Truncate any stale log from a previous run before QEMU starts, so the
  # wait loop cannot mistake an old success marker for this boot's.
  : >"$LOG_FILE"
  qemu-system-riscv64 \
    -machine virt \
    -smp "$SBI_SMP" \
    -m 512M \
    -nographic \
    -no-reboot \
    -bios "$RUSTSBI" \
    -kernel "$KERNEL_IMAGE" \
    -initrd "$INITRAMFS" \
    -dtb "$IDLE_DTB" \
    >"$LOG_FILE" 2>&1 &
  QEMU_PID=$!
}

start_qemu_uboot() {
  mkdir -p "$LOG_DIR"
  : >"$LOG_FILE"
  qemu-system-riscv64 \
    -machine virt \
    -smp 1 \
    -m 256M \
    -nographic \
    -no-reboot \
    -bios "$UB_SPL" \
    -device loader,file="$UB_ITB",addr=0x80200000 \
    -blockdev driver=file,filename="$DISK_IMAGE",node-name=hd0 \
    -device virtio-blk-device,drive=hd0 \
    >"$LOG_FILE" 2>&1 &
  QEMU_PID=$!
}

start_qemu_edk2() {
  mkdir -p "$LOG_DIR"
  : >"$LOG_FILE"
  qemu-system-riscv64 \
    -machine virt,pflash0=pflash0,pflash1=pflash1,acpi=off \
    -smp 1 \
    -m 2G \
    -nographic \
    -no-reboot \
    -bios "$RUSTSBI" \
    -blockdev "node-name=pflash0,driver=file,read-only=on,filename=${WORK_DIR}/RISCV_VIRT_CODE.fd" \
    -blockdev "node-name=pflash1,driver=file,filename=${WORK_DIR}/RISCV_VIRT_VARS.fd" \
    -kernel "$KERNEL_IMAGE" \
    -initrd "$INITRAMFS" \
    -append "console=ttyS0 earlycon=uart8250,mmio,0x10000000 rdinit=/init" \
    >"$LOG_FILE" 2>&1 &
  QEMU_PID=$!
}

userspace_is_ready() {
  grep -Fq "$SMOKE_MARKER" "$LOG_FILE"
}

boot_has_failed() {
  grep -Eq "$BOOT_FAILURE_PATTERN" "$LOG_FILE"
}

report_boot_failure() {
  echo "Linux failed to boot:" >&2
  grep -E --max-count=5 "$BOOT_FAILURE_PATTERN" "$LOG_FILE" >&2 || true
  tail -n 120 "$LOG_FILE" || true
}

report_early_exit() {
  local qemu_exit
  set +e
  wait "$QEMU_PID"
  qemu_exit=$?
  set -e

  echo "QEMU exited before Linux reached userspace (exit=${qemu_exit})" >&2
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

  echo "Linux did not reach userspace within ${BOOT_TIMEOUT_SECS}s" >&2
  tail -n 120 "$LOG_FILE" || true
  return 1
}

main() {
  trap cleanup EXIT

  check_prerequisites
  prepare_kernel
  prepare_busybox
  case "$BOOT_MODE" in
    u-boot)
      make_rootfs_image
      build_uboot
      start_qemu_uboot
      ;;
    edk2)
      build_initramfs
      ci_edk2_prepare "$WORK_DIR"
      start_qemu_edk2
      ;;
    sbi)
      build_initramfs
      prepare_idle_dtb
      start_qemu_sbi
      ;;
  esac
  wait_for_userspace

  echo "RustSBI booted Linux ${KERNEL_VERSION} to userspace successfully (${BOOT_MODE})"
  echo "QEMU log: ${LOG_FILE}"
}

main "$@"
