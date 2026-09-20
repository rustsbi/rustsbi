#!/usr/bin/env bash
set -euo pipefail

# Build a tiny guest instead of downloading a mutable distribution image.
# All three upstream source archives are pinned and checked before extraction.
cache_dir=${MINIMAL_LINUX_CACHE_DIR:-.minimal-linux/cache}
work_dir=${MINIMAL_LINUX_WORK_DIR:-.minimal-linux/work}
log_dir=${MINIMAL_LINUX_LOG_DIR:-qemu-logs}
rustsbi=${MINIMAL_LINUX_RUSTSBI:-target/riscv64gc-unknown-none-elf/release/rustsbi-prototyper.bin}
timeout_secs=${MINIMAL_LINUX_TIMEOUT_SECS:-90}
marker=RUSTSBI_MINIMAL_LINUX_USERSPACE_OK

kernel=linux-6.6.6.tar.xz
kernel_sha=ebf70a917934b13169e1be5b95c3b6c2fea5bc14e6dc144f1efb8a0016b224c8
busybox=busybox-1.36.1.tar.bz2
busybox_sha=b8cc24c9574d809e7279c3be349795c5d5ceb6fdf19ca709f80cde50e47de314
uboot=u-boot-25049ad560826f7dc1c4740883b0016014a59789.tar.gz
uboot_sha=18c2b8d88fc212c40891022144a9b3f7620356402c4f0296d8fdad25bad13bf9

mkdir -p "$cache_dir" "$work_dir" "$log_dir"
test -s "$rustsbi"

download() {
    local url=$1 file=$2 digest=$3
    if [ ! -s "$cache_dir/$file" ] || ! printf '%s  %s\n' "$digest" "$cache_dir/$file" | sha256sum --check --status; then
        rm -f "$cache_dir/$file"
        curl --fail --location --retry 5 --retry-delay 5 --output "$cache_dir/$file" "$url"
    fi
    printf '%s  %s\n' "$digest" "$cache_dir/$file" | sha256sum --check
}

download "https://cdn.kernel.org/pub/linux/kernel/v6.x/$kernel" "$kernel" "$kernel_sha"
download "https://busybox.net/downloads/$busybox" "$busybox" "$busybox_sha"
download "https://codeload.github.com/u-boot/u-boot/tar.gz/25049ad560826f7dc1c4740883b0016014a59789" "$uboot" "$uboot_sha"

kernel_dir="$work_dir/linux"
busybox_dir="$work_dir/busybox"
uboot_dir="$work_dir/u-boot"
rootfs="$work_dir/rootfs"
mkdir -p "$kernel_dir" "$busybox_dir" "$uboot_dir" "$rootfs"
tar -xf "$cache_dir/$kernel" -C "$kernel_dir" --strip-components=1
tar -xf "$cache_dir/$busybox" -C "$busybox_dir" --strip-components=1
tar -xf "$cache_dir/$uboot" -C "$uboot_dir" --strip-components=1

export ARCH=riscv CROSS_COMPILE=riscv64-linux-gnu-

make -C "$busybox_dir" defconfig
sed -i 's/^# CONFIG_STATIC is not set$/CONFIG_STATIC=y/' "$busybox_dir/.config"
# BusyBox 1.36.1's optional tc applet uses CBQ definitions removed from
# modern Linux UAPI headers. The initramfs does not need traffic control.
sed -i 's/^CONFIG_TC=y$/# CONFIG_TC is not set/' "$busybox_dir/.config"
# Ubuntu's RISC-V cross compiler may target newer optional extensions by
# default, while QEMU's virt CPU for this test exposes the portable GC set.
sed -i 's/^CONFIG_EXTRA_CFLAGS=".*"$/CONFIG_EXTRA_CFLAGS="-march=rv64gc -mabi=lp64d"/' "$busybox_dir/.config"
grep -qx 'CONFIG_STATIC=y' "$busybox_dir/.config"
grep -qx '# CONFIG_TC is not set' "$busybox_dir/.config"
grep -qx 'CONFIG_EXTRA_CFLAGS="-march=rv64gc -mabi=lp64d"' "$busybox_dir/.config"
make -C "$busybox_dir" -j"$(nproc)"
make -C "$busybox_dir" CONFIG_PREFIX="$(realpath "$rootfs")" install
riscv64-linux-gnu-readelf -A "$rootfs/bin/busybox" | grep Tag_RISCV_arch
mkdir -p "$rootfs/proc" "$rootfs/sys" "$rootfs/dev"
sudo mknod -m 600 "$rootfs/dev/console" c 5 1
sudo mknod -m 666 "$rootfs/dev/null" c 1 3
cat >"$rootfs/init" <<'INIT'
#!/bin/sh
set -eu
/bin/mount -t proc proc /proc
/bin/mount -t sysfs sysfs /sys
test "$(/bin/uname -m)" = riscv64
test -r /proc/version
printf '%s\n' RUSTSBI_MINIMAL_LINUX_USERSPACE_OK
/bin/sync
/bin/poweroff -f
INIT
chmod +x "$rootfs/init"

make -C "$kernel_dir" defconfig
"$kernel_dir/scripts/config" --file "$kernel_dir/.config" --enable BLK_DEV_INITRD
"$kernel_dir/scripts/config" --file "$kernel_dir/.config" --set-str INITRAMFS_SOURCE "$(realpath "$rootfs")"
make -C "$kernel_dir" olddefconfig
make -C "$kernel_dir" -j"$(nproc)" Image
image="$kernel_dir/arch/riscv/boot/Image"
test -s "$image"

make -C "$uboot_dir" qemu-riscv64_spl_defconfig
"$uboot_dir/scripts/config" --file "$uboot_dir/.config" --set-str BOOTCOMMAND \
    'setenv bootargs console=ttyS0 rdinit=/init; booti 0x88000000 - ${fdtcontroladdr}'
make -C "$uboot_dir" olddefconfig
# Use the distro's dtc and pylibfdt. The pinned U-Boot release's bundled
# SWIG wrapper does not build with the newer SWIG on current CI runners.
make -C "$uboot_dir" DTC=/usr/bin/dtc PYTHON3=/usr/bin/python3 \
    OPENSBI="$(realpath "$rustsbi")" -j"$(nproc)"
test -s "$uboot_dir/spl/u-boot-spl"
test -s "$uboot_dir/u-boot.itb"

log_file="$log_dir/minimal-linux.log"
# Ubuntu's static RISC-V libc contains vector routines even when BusyBox
# itself is compiled for rv64gc, so the virtual CPU must expose V.
set +e
timeout --foreground "${timeout_secs}s" qemu-system-riscv64 \
    -machine virt -cpu rv64,v=true -m 1024 -smp 1 -nographic -no-reboot \
    -bios "$uboot_dir/spl/u-boot-spl" \
    -device "loader,file=$uboot_dir/u-boot.itb,addr=0x80200000" \
    -device "loader,file=$image,addr=0x88000000" \
    >"$log_file" 2>&1
status=$?
set -e

tail -n 120 "$log_file"
if [ "$status" -ne 0 ]; then
    echo "QEMU exited with status $status" >&2
    exit 1
fi
if grep -Eiq 'Kernel panic|not syncing|U-Boot SPL.*error|Bad Linux RISC-V Image' "$log_file"; then
    echo 'Boot failure found in serial log' >&2
    exit 1
fi
grep -Fq "$marker" "$log_file" || {
    echo "Userspace success marker was not printed: $marker" >&2
    exit 1
}
echo "Minimal Linux boot log: $log_file"
