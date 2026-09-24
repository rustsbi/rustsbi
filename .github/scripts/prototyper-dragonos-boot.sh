#!/usr/bin/env bash
# Boot DragonOS through bare RustSBI or U-Boot, then run a BusyBox
# shell command through virtconsole. Requires `cargo prototyper build`, Docker and
# the packages in dragonos.yml. Usage: $0 [sbi|u-boot].
# The pinned DragonOS fork carries RISC-V userspace fixes pending upstream.
# Its default RISC-V init only prints Hello; use the existing BusyBox shell
# instead. Sources and immutable builds are cached; disks are recreated.
# See rustsbi/rustsbi#307 and #331 for the required coverage.
set -euo pipefail

mode=${1:-sbi}
case "$mode" in
  sbi|u-boot) ;;
  *) echo "Usage: $0 [sbi|u-boot]" >&2; exit 2 ;;
esac
(( $# <= 1 )) || { echo "Expected at most one boot mode" >&2; exit 2; }
cd "$(dirname "${BASH_SOURCE[0]}")/../.."
if [[ "$mode" == sbi ]]; then
  readonly rustsbi=target/riscv64gc-unknown-none-elf/release/rustsbi-prototyper-dynamic.elf
else
  readonly rustsbi=target/riscv64gc-unknown-none-elf/release/rustsbi-prototyper-dynamic.bin
fi
readonly DRAGONOS_REV=40572b4554bee5b0a46fb0eff17fdff3a4d74b5e
readonly STUB_REV=8515606674058ca81cd1c0b99453e326875c5c0d
readonly DRAGONOS_IMAGE=dragonos/dragonos-dev@sha256:de57dc949325dc94379defe710d973ba6531c322770d95aa265dd26709b0780a
readonly UBOOT_VERSION=2024.04
readonly UBOOT_SHA256=d6b57ce574a0a0504a5b6596644ceacb7f77bde9353779bcf2fde07c4b9a2b92
readonly BUSYBOX_VERSION=1.35.0
readonly BUSYBOX_SHA256=faeeb244c35a348a334f4a59e44626ee870fb07b6884d68c10ae8bc19f83a694
readonly MUSL_URL=https://github.com/DragonOS-Community/musl-cross-make/releases/download/9.4.0-231114/riscv64-linux-musl-cross-gcc-9.4.0.tar.xz
readonly MUSL_SHA256=b3833579b91138e496d4bcbee74fa744cf3bff743e9b6f9d1094fb7c3057a93a
readonly work_dir="$(realpath -m "${DRAGONOS_WORK_DIR:-.dragonos/work}")"
readonly cache_dir="$(realpath -m "${DRAGONOS_CACHE_DIR:-.cache/dragonos-bootloaders}")"
readonly source_dir="${DRAGONOS_SOURCE_DIR:-$work_dir/DragonOS}"
readonly run_dir="$(realpath -m "${DRAGONOS_LOG_DIR:-qemu-logs/dragonos}/$mode")"
readonly download_timeout="${DRAGONOS_DOWNLOAD_TIMEOUT_SECS:-900}"
readonly connect_timeout="${DRAGONOS_DOWNLOAD_CONNECT_TIMEOUT_SECS:-30}"

download_asset() {
  local url=$1 destination=$2 digest=$3 temporary
  if [[ -f "$destination" ]] && printf '%s  %s\n' "$digest" "$destination" | sha256sum --check --status; then
    return
  fi
  temporary=$(mktemp "${destination}.part.XXXXXX")
  if ! curl --fail --location --retry 3 --retry-all-errors \
    --connect-timeout "$connect_timeout" --max-time "$download_timeout" \
    --retry-max-time "$download_timeout" --output "$temporary" "$url" ||
    ! printf '%s  %s\n' "$digest" "$temporary" | sha256sum --check; then
    rm -f "$temporary"
    return 1
  fi
  mv "$temporary" "$destination"
}

prepare_dragonos() {
  if [[ ! -s "$cache_dir/bootriscv64.efi" || ! -s "$cache_dir/dragonos-kernel.bin" ]]; then
    if [[ ! -d "$source_dir/.git" ]]; then
      git init --quiet "$source_dir"
      git -C "$source_dir" remote add origin https://github.com/Pneuma-zy/DragonOS.git
      timeout "$download_timeout" git -C "$source_dir" fetch --quiet --depth=1 origin "$DRAGONOS_REV"
      git -C "$source_dir" checkout --quiet --detach FETCH_HEAD
    fi
    actual=$(git -C "$source_dir" rev-parse HEAD)
    [[ $actual == "$DRAGONOS_REV" ]] || {
      echo "DragonOS revision mismatch: $actual" >&2
      exit 1
    }
    timeout "$download_timeout" git -C "$source_dir" submodule update --init --depth=1 kernel/submodules/DragonStub
    actual=$(git -C "$source_dir/kernel/submodules/DragonStub" rev-parse HEAD)
    [[ $actual == "$STUB_REV" ]] || {
      echo "DragonStub revision mismatch: $actual" >&2
      exit 1
    }

    # The verified S-mode boot uses the original DragonStub, packed around the
    # kernel ELF. The image digest fixes both the Rust and GCC 11 toolchains.
    if ! docker image inspect "$DRAGONOS_IMAGE" >/dev/null 2>&1; then
      timeout "$download_timeout" docker pull "$DRAGONOS_IMAGE"
    fi
    docker run --rm --volume "$(realpath "$source_dir"):/work" \
      --volume /usr/riscv64-linux-gnu:/usr/riscv64-linux-gnu:ro \
      --workdir /work --entrypoint /bin/bash "$DRAGONOS_IMAGE" -lc '
        set -euo pipefail
        export CARGO_BUILD_JOBS=4
        make ARCH=riscv64 ROOTFS_MANIFEST=default NPROCS=4 kernel
        test -s bin/kernel/kernel.elf
        test -s bin/sysroot/efi/boot/bootriscv64.efi
      '
    cp "$source_dir/bin/sysroot/efi/boot/bootriscv64.efi" "$cache_dir/bootriscv64.efi"
    # DragonOS links below QEMU virt RAM. As in the teammate's bare path, QEMU
    # must receive a flattened image so it can place it at 0x80200000.
    rust-objcopy -O binary --binary-architecture=riscv64 \
      "$source_dir/bin/kernel/kernel.elf" "$cache_dir/dragonos-kernel.bin"
  fi
}

prepare_busybox() {
  [[ -s "$cache_dir/busybox" ]] && return
  local tarball="$work_dir/busybox-${BUSYBOX_VERSION}.tar.bz2"
  download_asset "$MUSL_URL" "$cache_dir/riscv64-linux-musl-cross-gcc-9.4.0.tar.xz" "$MUSL_SHA256"
  tar -xJf "$cache_dir/riscv64-linux-musl-cross-gcc-9.4.0.tar.xz" -C "$work_dir"
  download_asset "https://mirrors.dragonos.org.cn/pub/third_party/busybox/busybox-${BUSYBOX_VERSION}.tar.bz2" \
    "$tarball" "$BUSYBOX_SHA256"
  tar -xjf "$tarball" -C "$work_dir"
  local tree="$work_dir/busybox-${BUSYBOX_VERSION}"
  make -C "$tree" defconfig
  # Match DragonOS's static BusyBox build; TC needs removed Linux CBQ headers.
  sed -i -e 's/# CONFIG_STATIC is not set/CONFIG_STATIC=y/' \
    -e 's/CONFIG_TC=y/# CONFIG_TC is not set/' "$tree/.config"
  make -C "$tree" CROSS_COMPILE="$work_dir/riscv64-linux-musl-cross-gcc-9.4.0/bin/riscv64-linux-musl-" \
    EXTRA_CFLAGS='-idirafter /usr/riscv64-linux-gnu/include' -j"$(nproc)"
  cp "$tree/busybox" "$cache_dir/busybox"
}

prepare_bootloader() {
  if [[ "$mode" == u-boot ]]; then
    if [[ ! -s "$cache_dir/u-boot.bin" ]]; then
      if [[ -n ${DRAGONOS_UBOOT_SOURCE_DIR:-} ]]; then
        tree=$DRAGONOS_UBOOT_SOURCE_DIR
        [[ $(git -C "$tree" rev-parse HEAD) == 25049ad560826f7dc1c4740883b0016014a59789 ]]
      else
        tarball="$work_dir/u-boot-${UBOOT_VERSION}.tar.gz"
        download_asset "https://github.com/u-boot/u-boot/archive/refs/tags/v${UBOOT_VERSION}.tar.gz" \
          "$tarball" "$UBOOT_SHA256"
        tar -xzf "$tarball" -C "$work_dir"
        tree="$work_dir/u-boot-${UBOOT_VERSION}"
      fi
      make -C "$tree" O="$work_dir/uboot-build" ARCH=riscv \
        CROSS_COMPILE=riscv64-linux-gnu- qemu-riscv64_smode_defconfig
      make -C "$tree" O="$work_dir/uboot-build" ARCH=riscv \
        CROSS_COMPILE=riscv64-linux-gnu- -j"$(nproc)"
      cp "$work_dir/uboot-build/u-boot.bin" "$cache_dir/u-boot.bin"
    fi
  fi
}

# Both bootloaders and root=/dev/vda1 use this per-run partitioned FAT disk.
# mtools avoids privileged loop mounts. An empty volume label avoids a known
# FAT directory bug in the pinned DragonOS commit.
make_disk() {
  local disk="$run_dir/disk.img"
  fat=$(mktemp "${disk}.fat.XXXXXX")
  rm -f "$disk"
  truncate -s 2147483648 "$disk"
  printf 'label: dos\nunit: sectors\n\nstart=2048, size=4192256, type=c, bootable\n' \
    | sfdisk "$disk" >/dev/null
  truncate -s 2146435072 "$fat"
  # The pinned DragonOS commit does not yet include the FAT volume-label fix.
  # An empty label leaves the root directory without a volume-label entry.
  mkfs.fat -F 32 -S 512 -h 2048 --invariant -n '' "$fat" >/dev/null
  mmd -i "$fat" ::/efi ::/efi/boot ::/bin
  mcopy -i "$fat" "$cache_dir/bootriscv64.efi" ::/efi/boot/bootriscv64.efi
  mcopy -i "$fat" "$cache_dir/busybox" ::/bin/busybox
  mcopy -i "$fat" "$cache_dir/busybox" ::/bin/sh
  dd if="$fat" of="$disk" bs=1M seek=1 conv=notrunc,sparse status=none
  mdir -i "${disk}@@1048576" ::/efi/boot/bootriscv64.efi
  mdir -i "${disk}@@1048576" ::/bin/sh
  rm -f "$fat"
}

# QEMU's pipe backends provide stdin for U-Boot and the guest shell; output
# goes straight to logs. No terminal automation or custom guest protocol.
start_qemu() {
  local channel
  for channel in uart guest; do
    mkfifo "$run_dir/$channel.in" "$run_dir/$channel.out"
  done
  exec 3<>"$run_dir/uart.in" 4<>"$run_dir/guest.in"
  local -a args=(-machine virt -accel tcg -m 2G -smp 1 -no-reboot
    -display none -monitor none -nic none -bios "$rustsbi"
    -chardev "pipe,id=uart,path=$run_dir/uart" -serial chardev:uart
    -chardev "pipe,id=guest,path=$run_dir/guest"
    -device virtio-serial-device -device 'virtconsole,chardev=guest'
    -drive "if=none,id=hd0,format=raw,file=$run_dir/disk.img"
    -device 'virtio-blk-device,drive=hd0')
  case "$mode" in
    sbi) args+=(-kernel "$cache_dir/dragonos-kernel.bin" -append "$bootargs") ;;
    u-boot) args+=(-kernel "$cache_dir/u-boot.bin") ;;
  esac
  for channel in uart guest; do
    cat "$run_dir/$channel.out" >"$run_dir/$channel.log" &
    readers+=("$!")
  done
  qemu-system-riscv64 "${args[@]}" >"$run_dir/qemu.log" 2>&1 &
  qemu_pid=$!
  deadline=$((SECONDS + ${DRAGONOS_BOOT_TIMEOUT_SECS:-180}))
}

wait_for() {
  local log=$1 pattern=$2
  while (( SECONDS < deadline )); do
    if grep -Eq 'Kernel panic|Kernel Panic Occurred|panicked at|Unhandled exception:|EXCEPT_RISCV_ILLEGAL_INST|Synchronous Exception|do_trap_(insn|load|store)_page_fault' "$run_dir/"{uart,guest}.log; then
      echo 'DragonOS boot failed' >&2
      return 1
    fi
    grep -Eq "$pattern" "$log" && return
    if ! kill -0 "$qemu_pid" 2>/dev/null; then
      echo 'QEMU exited before the smoke test completed' >&2
      return 1
    fi
    sleep 0.2
  done
  echo "Timed out waiting for $pattern in $log" >&2
  return 1
}

boot_userspace() {
  wait_for "$run_dir/uart.log" 'Hello RustSBI!'
  if [[ "$mode" == u-boot ]]; then
    wait_for "$run_dir/uart.log" 'Hit any key to stop autoboot:'
    printf '\r' >&3
    wait_for "$run_dir/uart.log" '=> '
    # U-Boot expands fdtcontroladdr after receiving this command.
    # shellcheck disable=SC2016
    printf '%s' 'virtio scan; ' \
      'fatload virtio 0:1 0x84000000 /efi/boot/bootriscv64.efi; ' \
      'setenv bootargs; fdt move ${fdtcontroladdr} 0x88000000 0x10000; ' \
      'fdt addr 0x88000000; ' "fdt set /chosen bootargs \"$bootargs\"; " \
      'bootefi 0x84000000 0x88000000' $'\r' >&3
  fi
  if [[ "$mode" != sbi ]]; then
    wait_for "$run_dir/uart.log" 'Booting DragonOS kernel'
  fi
  wait_for "$run_dir/guest.log" '# '
  # Split the marker so terminal command echo cannot satisfy the assertion.
  printf '%s\n' "test -r /bin/sh && printf 'RUSTSBI-%s\\n' SMOKE-OK" >&4
  wait_for "$run_dir/guest.log" '^RUSTSBI-SMOKE-OK[[:space:]]*$'
  echo "DragonOS userspace smoke passed ($mode)"
}

cleanup() {
  local status=$? pid
  for pid in "${qemu_pid:-}" "${readers[@]}"; do
    [[ -n "$pid" ]] || continue
    kill "$pid" 2>/dev/null || true
    wait "$pid" 2>/dev/null || true
  done
  rm -f "$run_dir/"{uart,guest}.{in,out} "${fat:-}"
  if (( status != 0 )); then
    tail -n 80 "$run_dir/"*.log >&2 || true
  fi
}

main() {
  mkdir -p "$work_dir" "$cache_dir" "$run_dir"
  rm -f "$run_dir/"{uart.log,guest.log,qemu.log}
  exec > >(tee "$run_dir/build.log") 2>&1
  readers=()
  trap cleanup EXIT
  trap 'exit 130' INT
  trap 'exit 143' TERM
  test -s "$rustsbi" || { echo "Missing $rustsbi; run cargo prototyper build" >&2; exit 1; }
  qemu-system-riscv64 --version
  prepare_dragonos
  prepare_busybox
  prepare_bootloader
  make_disk
  bootargs='root=/dev/vda1 console=/dev/hvc0 init=/bin/sh rw -- -i'
  start_qemu
  boot_userspace
}

main "$@"
