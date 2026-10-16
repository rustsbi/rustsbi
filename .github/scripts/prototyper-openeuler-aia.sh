#!/usr/bin/env bash
set -euo pipefail

# Use the same release mirror and hashes as prototyper-openeuler-boot.sh.
base_url=${OPENEULER_AIA_BASE_URL:-https://mirror.maeen.sa/openeuler/openEuler-25.09/virtual_machine_img/riscv64}
image_name=${OPENEULER_AIA_IMAGE_NAME:-openEuler-25.09-riscv64.qcow2}
image_archive=${OPENEULER_AIA_IMAGE_ARCHIVE:-${image_name}.xz}
image_sha256=${OPENEULER_AIA_IMAGE_SHA256:-5ba2eb5f0dac1594e8e041e36125fc6e5a2bd4547b4039258e5bd5139fa067ba}
code_sha256=${OPENEULER_AIA_CODE_SHA256:-58e96e55744516ed5debff8ed90eaface7b8a51a9466e7c0a687f4025ed5c600}
vars_sha256=${OPENEULER_AIA_VARS_SHA256:-ea8094e953b1215444bd001ee1cf22818f1f7f8abcb158e62180cbaa6c1f70af}
cache_dir=${OPENEULER_AIA_CACHE_DIR:-.openeuler-aia/cache}
work_dir=${OPENEULER_AIA_WORK_DIR:-.openeuler-aia/work}
log_dir=${OPENEULER_AIA_LOG_DIR:-qemu-logs}
timeout_secs=${OPENEULER_AIA_TIMEOUT_SECS:-900}
smp=${OPENEULER_AIA_SMP:-4}
memory_gib=${OPENEULER_AIA_MEMORY_GIB:-4}
ssh_port=${OPENEULER_AIA_SSH_PORT:-12055}
rustsbi=${OPENEULER_AIA_RUSTSBI:-target/riscv64gc-unknown-none-elf/release/rustsbi-prototyper}

mkdir -p "$cache_dir" "$work_dir" "$log_dir"

download() {
    local url=$1
    local dest=$2
    local digest=$3
    local temp

    if [ -s "$dest" ] && printf '%s  %s\n' "$digest" "$dest" | sha256sum --check --status; then
        echo "Using verified $(basename "$dest")" >&2
        return
    fi

    echo "Downloading $url" >&2
    temp=$(mktemp "${dest}.part.XXXXXX") || return 1
    if ! curl --fail --location --connect-timeout 30 --max-time 1200 \
        --retry 2 --retry-all-errors --retry-max-time 1200 --output "$temp" "$url" \
        || ! printf '%s  %s\n' "$digest" "$temp" | sha256sum --check --status; then
        rm -f "$temp"
        echo "Failed to download verified asset: $url" >&2
        return 1
    fi
    mv "$temp" "$dest"
}

prepare_downloaded_image() {
    local archive_path="$cache_dir/$image_archive"
    local temp
    image_path="$work_dir/$image_name"

    download "$base_url/RISCV_VIRT_CODE.fd" "$cache_dir/RISCV_VIRT_CODE.fd" "$code_sha256" || return 1
    download "$base_url/RISCV_VIRT_VARS.fd" "$cache_dir/RISCV_VIRT_VARS.fd" "$vars_sha256" || return 1
    download "$base_url/$image_archive" "$archive_path" "$image_sha256" || return 1

    # Publish only a complete extraction, even after an interrupted earlier run.
    temp=$(mktemp "${image_path}.part.XXXXXX") || return 1
    case "$image_archive" in
        *.zst)
            zstd --decompress --stdout "$archive_path" >"$temp" || { rm -f "$temp"; return 1; }
            ;;
        *.xz)
            xz --decompress --stdout "$archive_path" >"$temp" || { rm -f "$temp"; return 1; }
            ;;
        *)
            rm -f "$temp"
            echo "Unsupported image archive: $image_archive" >&2
            return 1
            ;;
    esac
    mv "$temp" "$image_path"
}

image_path=${OPENEULER_AIA_IMAGE_PATH:-}
code_fd=${OPENEULER_AIA_CODE_FD:-}
vars_fd=${OPENEULER_AIA_VARS_FD:-}

if [ -z "$image_path" ]; then
    prepare_downloaded_image
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
overlay="$work_dir/openeuler-aia-overlay.qcow2"
log_file="$log_dir/openeuler-aia.log"

cp "$vars_fd" "$vars_copy"
rm -f "$overlay"
qemu-img create -f qcow2 -F qcow2 -b "$image_abs" "$overlay"

echo "Booting openEuler with RustSBI Prototyper + AIA"
echo "RustSBI: $rustsbi"
echo "Image:   $image_abs"
echo "Log:     $log_file"

set +e
timeout --foreground "${timeout_secs}s" qemu-system-riscv64 \
    -nographic \
    -machine virt,pflash0=pflash0,pflash1=pflash1,acpi=off,aia=aplic-imsic \
    -smp "$smp" \
    -m "${memory_gib}G" \
    -bios "$rustsbi" \
    -blockdev node-name=pflash0,driver=file,read-only=on,filename="$code_abs" \
    -blockdev node-name=pflash1,driver=file,filename="$vars_copy" \
    -drive file="$overlay",format=qcow2,id=hd0,if=none \
    -object rng-random,filename=/dev/urandom,id=rng0 \
    -device virtio-vga \
    -device virtio-rng-device,rng=rng0 \
    -device virtio-blk-device,drive=hd0 \
    -device virtio-net-device,netdev=usernet \
    -netdev user,id=usernet,hostfwd=tcp::"$ssh_port"-:22 \
    -device qemu-xhci \
    -usb \
    -device usb-kbd \
    -device usb-tablet \
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
    if grep -Eq "Kernel panic|panic|FAILED|SystemFailure|Invalid data" "$log_file"; then
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
grep -F "automatically in 0s" "$log_file"
grep -F "Loading Linux" "$log_file"
grep -F "Loading initial ramdisk" "$log_file"
grep -F "localhost login:" "$log_file"
if grep -Eq "Kernel panic|panic|FAILED|SystemFailure|Invalid data" "$log_file"; then
    exit 1
fi

echo "openEuler AIA boot log: $log_file"
