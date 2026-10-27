#!/usr/bin/env bash
# Boot openEuler 25.09 on the QEMU RISC-V server platform reference board
# (`riscv-server-ref`) through RustSBI Prototyper and EDK2.
#
# Storage split: EDK2/GRUB read the ESP through a virtio-blk-pci boot disk,
# while Linux roots from an AHCI disk. virtio-pci data path is avoided in
# Linux (initcall_blacklist) because its MSI path does not complete on this
# QEMU/kernel combination yet; see the rvsp-ref bring-up notes on #161.
set -euo pipefail

base_url=${OPENEULER_RVSP_BASE_URL:-https://repo.openeuler.org/openEuler-25.09/virtual_machine_img/riscv64}
image_name=${OPENEULER_RVSP_IMAGE_NAME:-openEuler-25.09-riscv64.qcow2}
image_archive=${OPENEULER_RVSP_IMAGE_ARCHIVE:-${image_name}.xz}
cache_dir=${OPENEULER_RVSP_CACHE_DIR:-.openeuler-rvsp/cache}
work_dir=${OPENEULER_RVSP_WORK_DIR:-.openeuler-rvsp/work}
log_dir=${OPENEULER_RVSP_LOG_DIR:-qemu-logs}
timeout_secs=${OPENEULER_RVSP_TIMEOUT_SECS:-900}
smp=${OPENEULER_RVSP_SMP:-4}
memory_gib=${OPENEULER_RVSP_MEMORY_GIB:-4}
qemu=${OPENEULER_RVSP_QEMU:-qemu-system-riscv64}
rustsbi=${OPENEULER_RVSP_RUSTSBI:-target/riscv64gc-unknown-none-elf/release/rustsbi-prototyper-dynamic.elf}

mkdir -p "$cache_dir" "$work_dir" "$log_dir"

download() {
    local url=$1
    local dest=$2

    if [ -s "$dest" ]; then
        echo "Using cached $(basename "$dest")" >&2
        return
    fi

    echo "Downloading $url" >&2
    curl --fail --location --retry 5 --retry-delay 5 --output "$dest" "$url"
}

# The ESP's grub.cfg gains the virtio-pci blacklist before Linux boots.
# The image's ESP spells the root directory "efi" in its long name; probe
# both spellings so either layout works.
prepare_boot_disk() {
    local image_path=$1
    local boot_raw="$work_dir/boot.raw"
    local esp="$work_dir/esp.img"
    local grub_cfg_path
    local grub_new="$work_dir/grub.cfg"

    if [ -s "$work_dir/overlay-boot.qcow2" ]; then
        echo "Using cached boot overlay" >&2
        return
    fi

    echo "Preparing patched boot disk" >&2
    qemu-img convert -O raw "$image_path" "$boot_raw"

    # Partition 1 of the image is the 2 GiB FAT ESP starting at sector 2048.
    dd if="$boot_raw" of="$esp" bs=512 skip=2048 count=4192256 status=none
    for grub_cfg_path in ::/efi/EFI/openEuler/grub.cfg ::/EFI/openEuler/grub.cfg; do
        if mtype -i "$esp" "$grub_cfg_path" >"$work_dir/grub.orig.cfg" 2>/dev/null; then
            break
        fi
    done
    grep -q "menuentry" "$work_dir/grub.orig.cfg"
    # Drop `quiet` so the root-mount marker reaches the serial log.
    sed -e 's/earlycon/earlycon initcall_blacklist=virtio_pci_driver_init/' \
        -e 's/ quiet//' \
        "$work_dir/grub.orig.cfg" >"$grub_new"
    grep -q initcall_blacklist "$grub_new"
    mcopy -i "$esp" -o "$grub_new" "$grub_cfg_path"
    dd if="$esp" of="$boot_raw" bs=512 seek=2048 conv=notrunc status=none

    qemu-img convert -f raw -O qcow2 "$boot_raw" "$work_dir/overlay-boot.qcow2"
    rm -f "$boot_raw" "$esp"
}

prepare_downloaded_image() {
    local archive_path="$cache_dir/$image_archive"
    local checksum_path="$cache_dir/$image_archive.sha256sum"
    local image_path="$work_dir/$image_name"

    download "$base_url/RISCV_VIRT_CODE.fd" "$cache_dir/RISCV_VIRT_CODE.fd"
    download "$base_url/RISCV_VIRT_VARS.fd" "$cache_dir/RISCV_VIRT_VARS.fd"
    download "$base_url/$image_archive" "$archive_path"
    download "$base_url/$image_archive.sha256sum" "$checksum_path"

    (
        cd "$cache_dir"
        sha256sum -c "$(basename "$checksum_path")"
    ) >&2

    if [ ! -s "$image_path" ]; then
        xz --decompress --force --keep --stdout "$archive_path" >"$image_path"
    fi

    printf '%s\n' "$image_path"
}

image_path=${OPENEULER_RVSP_IMAGE_PATH:-}
code_fd=${OPENEULER_RVSP_CODE_FD:-}
vars_fd=${OPENEULER_RVSP_VARS_FD:-}

if [ -z "$image_path" ]; then
    image_path=$(prepare_downloaded_image)
fi
if [ -z "$code_fd" ]; then
    code_fd="$cache_dir/RISCV_VIRT_CODE.fd"
fi
if [ -z "$vars_fd" ]; then
    vars_fd="$cache_dir/RISCV_VIRT_VARS.fd"
fi

test -s "$rustsbi"
test -s "$image_path"
test -s "$code_fd"
test -s "$vars_fd"

image_abs=$(realpath "$image_path")
code_abs=$(realpath "$code_fd")
vars_copy="$work_dir/RISCV_VIRT_VARS-ci.fd"
overlay_root="$work_dir/overlay-root.qcow2"
log_file="$log_dir/openeuler-rvsp.log"

cp "$vars_fd" "$vars_copy"
rm -f "$overlay_root"
qemu-img create -f qcow2 -F qcow2 -b "$image_abs" "$overlay_root" >/dev/null
prepare_boot_disk "$image_abs"

echo "Booting openEuler on riscv-server-ref with RustSBI Prototyper"
echo "RustSBI: $rustsbi"
echo "QEMU:    $($qemu --version | head -1)"
echo "Log:     $log_file"

set +e
timeout --foreground "${timeout_secs}s" "$qemu" \
    -nographic \
    -machine riscv-server-ref,pflash0=pf0,pflash1=pf1 \
    -smp "$smp" \
    -m "${memory_gib}G" \
    -bios "$rustsbi" \
    -blockdev node-name=pf0,driver=file,read-only=on,filename="$code_abs" \
    -blockdev node-name=pf1,driver=file,filename="$vars_copy" \
    -device virtio-blk-pci,drive=hd0 \
    -drive id=hd0,if=none,format=qcow2,file="$work_dir/overlay-boot.qcow2" \
    -device ahci,id=ahci0 \
    -device ide-hd,drive=hd1,bus=ahci0.0 \
    -drive id=hd1,if=none,format=qcow2,file="$overlay_root" \
    >"$log_file" 2>&1 &
qemu_pid=$!
set -e

cleanup() {
    if kill -0 "$qemu_pid" 2>/dev/null; then
        kill "$qemu_pid" 2>/dev/null || true
        wait "$qemu_pid" 2>/dev/null || true
    fi
}
trap cleanup EXIT

deadline=$((SECONDS + timeout_secs))
while kill -0 "$qemu_pid" 2>/dev/null; do
    if grep -Fq "localhost login:" "$log_file"; then
        break
    fi
    if grep -Eq "Kernel panic|FAILED|SystemFailure|Invalid data" "$log_file"; then
        tail -n 160 "$log_file" || true
        exit 1
    fi
    if [ "$SECONDS" -ge "$deadline" ]; then
        break
    fi
    sleep 2
done

cleanup
trap - EXIT

# Show QEMU startup errors before the boot-marker checks can fail.
tail -n 160 "$log_file"

grep -F "Platform IPI Extension        : IMSIC" "$log_file"
grep -F "Loading Linux" "$log_file"
grep -F "EXT4-fs (sda2): mounted filesystem" "$log_file"
grep -F "localhost login:" "$log_file"
if grep -Eq "Kernel panic|FAILED|SystemFailure|Invalid data" "$log_file"; then
    exit 1
fi

echo "openEuler rvsp-ref boot log: $log_file"
