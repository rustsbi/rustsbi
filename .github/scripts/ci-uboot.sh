#!/usr/bin/env bash
# Source this file from boot tests. Build U-Boot once per configuration, then
# package the current RustSBI separately with ci_uboot_fit.

CI_UBOOT_SCRIPT_PATH=$(realpath "${BASH_SOURCE[0]}")
CI_UBOOT_ARCEOS_PAYLOAD_ADDR=0x84000000

ci_uboot_root() {
  realpath -m "${CI_UBOOT_CACHE_DIR:-.cache/ci-uboot}"
}

# Prints: source REPOSITORY COMMIT, or package MIRROR SHA256.
ci_uboot_info() {
  local kind=source url=https://github.com/u-boot/u-boot.git pin
  case "$1" in
    2024.04) pin=25049ad560826f7dc1c4740883b0016014a59789 ;;
    2026.07) pin=ece349ade2973e220f524ce59e59711cc919263f ;;
    2025.10-0ubuntu0.24.04.2)
      kind=package
      url=https://snapshot.ubuntu.com/ubuntu/20261006T100000Z
      pin=2154e34e4c7037105e8448514faad296b2bbfb1cc460674e3d67cca42d402d9a ;;
    2025.10-0ubuntu0.24.04.3)
      kind=package
      url=https://snapshot.ubuntu.com/ubuntu/20261014T000000Z
      pin=852a1f5d4a5d00a3a63f694a4bf46744f02737f5ffdea091666ee44d784f5eba ;;
    *) echo "Unsupported U-Boot version: $1" >&2; return 2 ;;
  esac
  printf '%s %s %s\n' "$kind" "$url" "$pin"
}

ci_uboot_toolchain_id() {
  local cross=${CROSS_COMPILE:-riscv64-linux-gnu-}
  {
    cat /etc/os-release
    uname -m
    "${cross}gcc" --version
    "${cross}ld" --version
    gcc --version
  } | sha256sum | cut -d ' ' -f 1
}

ci_uboot_source() (
  set -euo pipefail
  local version=$1 kind repository revision uboot_source
  read -r kind repository revision < <(ci_uboot_info "$version") || exit 2
  [[ "$kind" == source ]] || exit 2
  uboot_source="$(ci_uboot_root)/sources/$version"
  if [[ $(git -C "$uboot_source" rev-parse HEAD 2>/dev/null) != "$revision" ]]; then
    mkdir -p "$uboot_source"
    git -C "$uboot_source" init --quiet || exit 1
    timeout 10m git -C "$uboot_source" fetch --quiet --depth=1 \
      "$repository" "$revision" || exit 1
    git -C "$uboot_source" checkout --quiet --detach FETCH_HEAD || exit 1
  fi
  printf '%s\n' "$uboot_source"
)

ci_uboot_valid_build() (
  cd "$1" 2>/dev/null || return 1
  test -s SHA256SUMS && sha256sum --check --status SHA256SUMS
)

# Usage: ci_uboot_build VERSION spl|smode PROFILE OUTPUT [scripts/config options]
ci_uboot_build() (
  set -euo pipefail
  local version=$1 mode=$2 profile=$3 destination=$4
  shift 4
  local kind revision cache_key build_cache uboot_source build_dir cross bootcmd
  local -a targets=(u-boot.bin tools)
  local -a commands=() options=()
  read -r kind _ revision < <(ci_uboot_info "$version") || exit 2
  [[ "$kind" == source ]] || exit 2
  cross=${CROSS_COMPILE:-riscv64-linux-gnu-}

  # U-Boot expands the quoted variables when it boots the guest.
  # shellcheck disable=SC2016
  case "$profile" in
    aosc-os-spl)
      commands=('setenv fdt_high' 'virtio scan'
        'sysboot virtio 0 any 0x84000000 /boot/extlinux/extlinux.conf') ;;
    arceos-smode)
      commands=("booti $CI_UBOOT_ARCEOS_PAYLOAD_ADDR - \${fdtcontroladdr}") ;;
    archlinux-spl)
      commands=('setenv fdt_high' 'virtio scan'
        'sysboot virtio 0:2 any 0x84000000 /boot/extlinux/extlinux.conf') ;;
    asterinas-spl)
      commands=('load virtio 0 ${kernel_addr_r} asterinas.uimg'
        'load virtio 0 ${ramdisk_addr_r} initramfs.cpio.gz'
        'setenv rd_size ${filesize}' 'fdt addr ${fdtcontroladdr}'
        'fdt move ${fdtcontroladdr} ${fdt_addr_r} 0x10000'
        'bootm ${kernel_addr_r} ${ramdisk_addr_r}:${rd_size} ${fdt_addr_r}') ;;
    eweos-smode)
      commands=('load virtio 0 ${kernel_addr_r} vmlinuz-linux'
        'load virtio 0 ${ramdisk_addr_r} initramfs-linux.img'
        'fdt move ${fdtcontroladdr} ${fdt_addr_r} 0x10000'
        'booti ${kernel_addr_r} ${ramdisk_addr_r}:${filesize} ${fdt_addr_r}') ;;
    minimal-linux-spl)
      commands=('ext4load virtio 0:1 84000000 Image'
        'setenv bootargs root=/dev/vda1 rw console=ttyS0'
        'booti 0x84000000 - ${fdtcontroladdr}') ;;
    nuttx-smode)
      commands=('virtio scan' 'load virtio 0 ${kernel_addr_r} nuttx.uimg'
        'fdt move ${fdtcontroladdr} ${fdt_addr_r} 0x10000'
        'bootm ${kernel_addr_r} - ${fdt_addr_r}') ;;
    polyos-spl)
      commands=('setenv fdt_high 0x88000000' 'run distro_bootcmd') ;;
    rtt-spl)
      commands=('load virtio 0 ${kernel_addr_r} rtthread.uimg'
        'fdt addr ${fdtcontroladdr}' 'fdt move ${fdtcontroladdr} ${fdt_addr_r} 0x10000'
        'bootm ${kernel_addr_r} - ${fdt_addr_r}')
      options+=(--set-val SPL_SYS_MALLOC_SIZE 0x800000) ;;
    zcore-spl)
      commands=('load virtio 0 ${kernel_addr_r} zcore.uimg'
        'load virtio 0 ${ramdisk_addr_r} rootfs.img'
        'fdt addr ${fdtcontroladdr}' 'fdt move ${fdtcontroladdr} ${fdt_addr_r} 0x10000'
        'bootm ${kernel_addr_r} ${ramdisk_addr_r}:${filesize} ${fdt_addr_r}') ;;
    spl-*|smode-*) ;;
    *) echo "Unknown U-Boot profile: $profile" >&2; exit 2 ;;
  esac
  # shellcheck disable=SC2016
  case "$profile" in
    asterinas-spl|eweos-smode|zcore-spl)
      # Kernel command lines are runtime data on the boot disk.
      commands=('virtio scan' 'load virtio 0 ${scriptaddr} uboot.env'
        'env import -t ${scriptaddr} ${filesize} bootargs' "${commands[@]}")
      options+=(--enable CMD_IMPORTENV) ;;
  esac
  if (( ${#commands[@]} )); then
    bootcmd=$(printf '%s; ' "${commands[@]}")
    options+=(--enable USE_BOOTCOMMAND --set-str BOOTCOMMAND "$bootcmd")
  fi
  options+=("$@")

  # Configuration or compiler changes need a different cached build.
  cache_key=$(
    {
      printf '%s\0' "$revision" "$mode" "$cross" "${options[@]}"
      ci_uboot_toolchain_id
      sha256sum < "$CI_UBOOT_SCRIPT_PATH"
    } | sha256sum | cut -d ' ' -f 1
  )
  build_cache="$(ci_uboot_root)/builds/$profile/$cache_key"
  if ci_uboot_valid_build "$build_cache"; then
    echo "Using compiled U-Boot $version ($profile)" >&2
  else
    uboot_source=$(ci_uboot_source "$version") || exit 1
    build_dir=$(mktemp -d "${RUNNER_TEMP:-/tmp}/rustsbi-uboot.XXXXXX")
    trap 'rm -rf "$build_dir"' EXIT
    make -C "$uboot_source" O="$build_dir" ARCH=riscv CROSS_COMPILE="$cross" \
      "qemu-riscv64_${mode}_defconfig" || exit 1
    if (( ${#options[@]} )); then
      "$uboot_source/scripts/config" --file "$build_dir/.config" "${options[@]}" || exit 1
    fi
    if [[ "$mode" == spl ]]; then
      targets+=(spl/u-boot-spl.bin)
    fi
    # Build binaries and tools only; the FIT gets the current RustSBI later.
    make -C "$uboot_source" O="$build_dir" ARCH=riscv CROSS_COMPILE="$cross" \
      -j"${CI_UBOOT_JOBS:-$(nproc)}" "${targets[@]}" || exit 1

    rm -rf "$build_cache"
    mkdir -p "$build_cache/tools"
    cp "$build_dir/.config" "$build_dir/u-boot.bin" "$build_dir/u-boot-nodtb.bin" \
      "$build_cache/" || exit 1
    cp "$build_dir/u-boot" "$build_cache/uboot.elf" || exit 1
    cp "$build_dir/tools/mkimage" "$build_dir/tools/dumpimage" "$build_dir/scripts/dtc/dtc" \
      "$build_cache/tools/" || exit 1
    if [[ -s "$build_dir/u-boot.dtb" ]]; then
      cp "$build_dir/u-boot.dtb" "$build_cache/" || exit 1
    fi
    if [[ "$mode" == spl ]]; then
      mkdir -p "$build_cache/spl"
      cp "$build_dir/spl/u-boot-spl" "$build_cache/spl/" || exit 1
    fi
    # Write the manifest last, so an interrupted build is never a cache hit.
    (
      cd "$build_cache" || exit 1
      find . -type f -exec sha256sum {} +
    ) > "$build_dir/SHA256SUMS" || exit 1
    mv "$build_dir/SHA256SUMS" "$build_cache/" || exit 1
    echo "Built U-Boot $version ($profile)" >&2
  fi
  mkdir -p "$destination"
  cp -a "$build_cache/." "$destination/"
)

# Usage: ci_uboot_fit OUTPUT RUSTSBI_BIN
# OUTPUT is the build copy, outside the shared cache.
ci_uboot_fit() (
  set -euo pipefail
  local destination firmware text_address firmware_address
  local fdt_image='' fdt_config=''
  destination=$(realpath "$1")
  firmware=$(realpath "$2")
  cd "$destination"
  text_address=$(sed -n 's/^CONFIG_TEXT_BASE=//p' .config)
  firmware_address=$(sed -n 's/^CONFIG_SPL_OPENSBI_LOAD_ADDR=//p' .config)

  if ! grep -qx 'CONFIG_OF_BOARD=y' .config; then
    fdt_image='    fdt-1 {
      description = "qemu-virt64";
      data = /incbin/("u-boot.dtb");
      type = "flat_dt";
      compression = "none";
    };'
    fdt_config='      fdt = "fdt-1";'
  fi
  cp "$firmware" rustsbi-dynamic.bin || exit 1
  cat >u-boot.its <<EOF
/dts-v1/;
/ {
  description = "Configuration to load RustSBI before U-Boot";
  #address-cells = <2>;
  images {
    uboot {
      description = "U-Boot";
      data = /incbin/("u-boot-nodtb.bin");
      type = "standalone";
      os = "U-Boot";
      arch = "riscv";
      compression = "none";
      load = /bits/ 64 <$text_address>;
    };
    rustsbi {
      description = "RustSBI dynamic firmware";
      data = /incbin/("rustsbi-dynamic.bin");
      type = "firmware";
      os = "opensbi";
      arch = "riscv";
      compression = "none";
      load = /bits/ 64 <$firmware_address>;
      entry = /bits/ 64 <$firmware_address>;
    };
$fdt_image
  };
  configurations {
    default = "conf-1";
    conf-1 {
      description = "qemu-virt64";
      firmware = "rustsbi";
      loadables = "uboot";
$fdt_config
    };
  };
};
EOF
  PATH="$destination/tools:$PATH" ./tools/mkimage -f u-boot.its u-boot.itb || exit 1
  ./tools/dumpimage -T flat_dt -p 1 -o rustsbi-from-fit.bin u-boot.itb >fit-layout.log || exit 1
  cmp "$firmware" rustsbi-from-fit.bin || exit 1
  echo "Verified u-boot.itb contains current-commit RustSBI" >&2
)

# Usage: ci_uboot_package VERSION OUTPUT
ci_uboot_package() (
  set -euo pipefail
  local version=$1 destination=$2 kind mirror digest package archive
  read -r kind mirror digest < <(ci_uboot_info "$version") || exit 2
  [[ "$kind" == package ]] || exit 2
  package="u-boot-qemu_${version}_all.deb"
  archive="$(ci_uboot_root)/packages/$version/$package"
  mkdir -p "$(dirname "$archive")" "$destination"
  if [[ ! -s "$archive" ]] || ! printf '%s  %s\n' "$digest" "$archive" | sha256sum -c --status; then
    curl --fail --location --connect-timeout 30 --max-time 300 \
      --retry 3 --retry-all-errors --output "$archive" \
      "$mirror/pool/main/u/u-boot/$package" || exit 1
    printf '%s  %s\n' "$digest" "$archive" | sha256sum -c --status || exit 1
  fi
  if command -v dpkg-deb >/dev/null 2>&1; then
    dpkg-deb -x "$archive" "$destination" || exit 1
  elif command -v ar >/dev/null 2>&1 && command -v bsdtar >/dev/null 2>&1; then
    ar p "$archive" data.tar.zst | bsdtar -xf - -C "$destination" || exit 1
  else
    echo 'Need dpkg-deb or ar + bsdtar to extract U-Boot' >&2
    exit 1
  fi
  test -s "$destination/usr/lib/u-boot/qemu-riscv64_smode/uboot.elf"
)
