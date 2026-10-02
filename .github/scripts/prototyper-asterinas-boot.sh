#!/usr/bin/env bash
#
# Boot Asterinas through RustSBI Prototyper in QEMU.
#
# Two boot paths are covered, selected by the first argument:
#
#   sbi     QEMU -> RustSBI (dynamic) -> Asterinas. The dynamic firmware follows
#           the fw_dynamic handoff convention, so QEMU starts the kernel
#           directly: `-bios` points at the firmware and `-kernel` at the
#           Asterinas ELF, with QEMU passing the device tree and entry point to
#           the firmware through the dynamic info structure.
#
#   u-boot  QEMU -> u-boot-spl -> RustSBI -> u-boot.itb -> Asterinas. The SPL
#           embeds RustSBI Prototyper as its OpenSBI payload, matching the
#           repository guide
#           firmware/docs/booting-linux-kernel-in-qemu-using-uboot-and-rustsbi.md.
#           Asterinas ships no Linux `Image`, so the kernel ELF is flattened
#           and wrapped in a legacy uImage that `bootm` starts with the Linux
#           boot protocol (hart ID in a0, device tree in a1).
#
# Both paths hand the same things to the kernel through the device tree:
# Asterinas reads its command line from `/chosen/bootargs` and its initramfs
# from `/chosen/linux,initrd-*`, which QEMU provides with `-append`/`-initrd`
# on the sbi path and U-Boot provides with bootargs and a raw ramdisk on the
# u-boot path.
#
# Asterinas publishes no prebuilt riscv64 boot artifacts, so it is built from a
# pinned commit inside its official development image. Only the build products
# (the kernel ELF and the bundle initramfs) and the resulting command line are
# cached, so a warm run skips the container build entirely; the image digest and
# the source commit are both checked before the build starts, and RustSBI is
# rebuilt from the commit under test and is never cached. Because the u-boot
# path embeds that firmware in the SPL, its U-Boot build is not cached either.
#
# Asterinas reads the `seed` CSR when the device tree advertises Zkr, and S-mode
# access is gated in M-mode by `mseccfg.SSEED`. RustSBI sets that bit since the
# Prototyper learned about Zkr, so `-cpu rv64,svpbmt=true,zkr=true` boots as
# Asterinas expects; without the bit the kernel takes an illegal instruction on
# its first read of `seed`.
#
# The command line is taken from the built bundle rather than guessed, and the
# kernel console is moved to `ttyS0` so the 16550 UART shared with RustSBI
# carries both the firmware and the kernel output over `-nographic` stdio.
# That keeps a single log sufficient to decide the boot.
#
# Requires: `cargo prototyper build` to have produced the firmware, plus docker
# (to run the Asterinas build image), git, readelf and qemu-system-riscv64. The
# u-boot path additionally needs the U-Boot build toolchain (riscv64-linux-gnu
# gcc/objcopy, swig, python3-dev, libssl-dev, bison, flex) plus mkfs.vfat and
# mcopy from dosfstools and mtools.

set -euo pipefail

if (( $# > 1 )); then
  echo "Usage: $0 [sbi|u-boot]" >&2
  exit 2
fi

readonly BOOT_MODE="${1:-sbi}"
case "$BOOT_MODE" in
  sbi | u-boot) ;;
  *)
    echo "Unknown boot path: ${BOOT_MODE}" >&2
    echo "Usage: $0 [sbi|u-boot]" >&2
    exit 2
    ;;
esac

# Asterinas is pinned to v0.18.1. Bump both the commit and the image together,
# then refresh the cache key in .github/workflows/asterinas.yml.
readonly ASTERINAS_URL="${ASTERINAS_URL:-https://github.com/asterinas/asterinas.git}"
readonly ASTERINAS_COMMIT="${ASTERINAS_COMMIT:-d924a9635a66c7c3bb43e563eaafa2c61d6ee9d5}"

# The development image carries the riscv64 cross toolchain, cargo-osdk's build
# inputs and Nix. Pull it through the registry manifest digest rather than the
# tag, so a re-pushed tag cannot silently change the guest under test.
readonly ASTERINAS_IMAGE="${ASTERINAS_IMAGE:-asterinas/kernel-dev:0.18.1-20260918}"
readonly ASTERINAS_IMAGE_DIGEST="${ASTERINAS_IMAGE_DIGEST:-sha256:55732d279d17280ecf730b6f92527b28b15540f7417fd0ada62f239d3828dfce}"
readonly ASTERINAS_IMAGE_REF="${ASTERINAS_IMAGE}@${ASTERINAS_IMAGE_DIGEST}"

readonly UB_VERSION="2024.04"
readonly UB_URL="https://github.com/u-boot/u-boot/archive/refs/tags/v${UB_VERSION}.tar.gz"
readonly UB_SHA256="d6b57ce574a0a0504a5b6596644ceacb7f77bde9353779bcf2fde07c4b9a2b92"
readonly CROSS_COMPILE="riscv64-linux-gnu-"

# The sbi path boots the dynamic firmware directly; the u-boot path embeds the
# flattened firmware as the OpenSBI payload of U-Boot's SPL.
if [[ "$BOOT_MODE" = u-boot ]]; then
  readonly RUSTSBI="${ASTERINAS_RUSTSBI_BIN:-target/riscv64gc-unknown-none-elf/release/rustsbi-prototyper.bin}"
  readonly BOOT_TIMEOUT_SECS="${ASTERINAS_BOOT_TIMEOUT_SECS:-420}"
else
  readonly RUSTSBI="${ASTERINAS_RUSTSBI:-target/riscv64gc-unknown-none-elf/release/rustsbi-prototyper-dynamic.elf}"
  readonly BOOT_TIMEOUT_SECS="${ASTERINAS_BOOT_TIMEOUT_SECS:-300}"
fi

readonly CACHE_DIR="${ASTERINAS_CACHE_DIR:-.cache/asterinas}"
readonly WORK_DIR="${ASTERINAS_WORK_DIR:-.asterinas/work}"
readonly LOG_DIR="${QEMU_LOG_DIR:-qemu-logs}"
readonly LOG_FILE="${LOG_DIR}/prototyper-asterinas-${BOOT_MODE}.log"

# Asterinas uses Svpbmt for page-table PBMT bits and reads `seed` under Zkr;
# both are what its own qemu_args.sh selects for riscv64.
readonly ASTERINAS_CPU="${ASTERINAS_CPU:-rv64,svpbmt=true,zkr=true}"
readonly ASTERINAS_MEMORY="${ASTERINAS_MEMORY:-2G}"
readonly ASTERINAS_SMP="${ASTERINAS_SMP:-1}"

readonly KERNEL_ELF="${CACHE_DIR}/aster-nix-boot.elf"
readonly INITRAMFS="${CACHE_DIR}/initramfs-boot.cpio.gz"
readonly CMDLINE_FILE="${CACHE_DIR}/cmdline.txt"

# U-Boot products. None of them are cached: the SPL embeds the firmware built
# from the commit under test, and the uImage and disk image are derived from it.
readonly ASTERINAS_KERNEL_BIN="${WORK_DIR}/asterinas-kernel.bin"
readonly ASTERINAS_UIMAGE="${WORK_DIR}/asterinas.uimg"
readonly BOOT_DISK="${WORK_DIR}/asterinas-boot.img"

# Populated by build_uboot; kept as mutable globals because the tree lives
# under WORK_DIR and is rebuilt every run.
UB_TREE=""
UB_SPL=""
UB_ITB=""

# The marker is printed by `/test/boot_hello.sh`, which the OSDK bundle runs as
# the init command when AUTO_TEST=boot. Matching the whole line keeps the same
# text echoed on a command line from satisfying the check. Asterinas runs the
# init command through `script`, so the line carries carriage returns on both
# ends; allow any number of them.
readonly SUCCESS_MARKER='Successfully booted\.'
# The u-boot path must also prove that the kernel came through U-Boot instead of
# merely reaching the same userspace by another route.
UB_SUCCESS_MARKERS=("U-Boot ${UB_VERSION}" "Starting kernel ...")
readonly UB_SUCCESS_MARKERS
# A panic halts the machine while QEMU stays alive, so without this the job
# would sit out the whole timeout and report only that the marker never
# appeared. The last two patterns are Asterinas's own wording.
readonly BOOT_FAILURE_PATTERN="Uncaught panic|Cannot handle kernel CPU exception|Kernel panic|not syncing|Attempted to kill init|SBI panic"
# U-Boot's own image and device tree failures.
readonly UB_BOOT_FAILURE_PATTERN="Wrong Image Format|Bad Linux RISCV Image magic|Ramdisk image is corrupt|Could not find a valid device tree|FDT creation failed|Failed to reserve memory for fdt"

QEMU_PID=""

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
    --connect-timeout 30 \
    --max-time 900 \
    --retry 3 \
    --retry-all-errors \
    --retry-max-time 900 \
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

# Check out the pinned commit without cloning the full history. The work tree is
# a build input and is never cached; only the products below are.
prepare_asterinas_source() {
  local source="${WORK_DIR}/asterinas"
  local head

  if [[ -d "${source}/.git" ]] && [[ "$(git -C "$source" rev-parse HEAD 2>/dev/null || true)" == "$ASTERINAS_COMMIT" ]]; then
    echo "Using checked out Asterinas ${ASTERINAS_COMMIT:0:8}" >&2
    return
  fi

  rm -rf "$source"
  mkdir -p "$source"

  git init --quiet "$source"
  git -C "$source" remote add origin "$ASTERINAS_URL"
  git -C "$source" fetch --quiet --depth=1 origin "$ASTERINAS_COMMIT"
  git -C "$source" checkout --quiet --detach FETCH_HEAD
  head=$(git -C "$source" rev-parse HEAD)
  if [[ "$head" != "$ASTERINAS_COMMIT" ]]; then
    echo "Asterinas checkout mismatch: expected ${ASTERINAS_COMMIT}, got ${head}" >&2
    return 1
  fi
}

# The guest image is built by `make kernel` inside the pinned container. The
# kernel ELF and initramfs are located through the bundle manifest instead of
# hardcoded names, and the command line is derived from the same manifest with
# the console moved to the 16550 UART shared with RustSBI.
write_container_script() {
  local destination=$1

  cat >"$destination" <<'CONTAINER'
#!/usr/bin/env bash
set -euo pipefail

cd /work
make kernel TARGET_ARCH=riscv64 RELEASE=1 AUTO_TEST=boot

python3 - <<'PY'
import pathlib
import shutil
import tomllib

bundle = pathlib.Path("/work/target/osdk/asterinas")
out = pathlib.Path("/out")

with open(bundle / "bundle.toml", "rb") as handle:
    manifest = tomllib.load(handle)

kernel = bundle / manifest["aster_bin"]["path"]
initramfs = bundle / manifest["initramfs"]["path"]
kcmdline = manifest["config"]["run"]["boot"]["kcmdline"]

shutil.copyfile(kernel, out / "aster-nix-boot.elf")
shutil.copyfile(initramfs, out / "initramfs-boot.cpio.gz")

append = " ".join("console=ttyS0" if arg == "console=hvc0" else arg for arg in kcmdline)
(out / "cmdline.txt").write_text(append + "\n")
PY
CONTAINER
  chmod +x "$destination"
}

build_asterinas() {
  local source="${WORK_DIR}/asterinas"
  local ci_dir="${WORK_DIR}/ci"
  local products="${WORK_DIR}/products"
  # Docker only accepts absolute host paths in `-v`, and the defaults here are
  # relative, so resolve each mount before handing it over.
  local source_abs ci_abs products_abs

  mkdir -p "$ci_dir" "$CACHE_DIR"
  write_container_script "${ci_dir}/build-asterinas.sh"
  prepare_asterinas_source

  source_abs="$(realpath -m "$source")"
  ci_abs="$(realpath -m "$ci_dir")"
  products_abs="$(realpath -m "$products")"

  # Referencing the image by digest makes the pull itself the integrity check:
  # docker refuses a digest that no longer resolves, and the tag cannot move the
  # bytes underneath it.
  echo "Pulling ${ASTERINAS_IMAGE_REF}" >&2
  docker pull "$ASTERINAS_IMAGE_REF" >&2

  rm -rf "$products"
  mkdir -p "$products"

  echo "Building Asterinas ${ASTERINAS_COMMIT:0:8} in ${ASTERINAS_IMAGE_REF}" >&2
  docker run --rm \
    -v "${source_abs}:/work" \
    -v "${ci_abs}:/ci:ro" \
    -v "${products_abs}:/out" \
    -w /work \
    "$ASTERINAS_IMAGE_REF" \
    bash /ci/build-asterinas.sh

  test -s "${products}/aster-nix-boot.elf" || {
    echo "Asterinas build produced no kernel ELF" >&2
    return 1
  }
  test -s "${products}/initramfs-boot.cpio.gz" || {
    echo "Asterinas build produced no initramfs" >&2
    return 1
  }
  grep -Fq 'console=ttyS0' "${products}/cmdline.txt" || {
    echo "Asterinas command line has no ttyS0 console" >&2
    return 1
  }

  # Publish the products only after the build fully succeeded, so a failed run
  # cannot leave a partial cache behind for the next one to trust.
  mv "${products}/aster-nix-boot.elf" "$KERNEL_ELF"
  mv "${products}/initramfs-boot.cpio.gz" "$INITRAMFS"
  mv "${products}/cmdline.txt" "$CMDLINE_FILE"
}

prepare_asterinas() {
  if [[ -s "$KERNEL_ELF" && -s "$INITRAMFS" && -s "$CMDLINE_FILE" ]]; then
    echo "Using cached Asterinas ${ASTERINAS_COMMIT:0:8} boot products" >&2
    return
  fi
  build_asterinas
}

stop_qemu() {
  if [[ -n "$QEMU_PID" ]] && kill -0 "$QEMU_PID" 2>/dev/null; then
    kill "$QEMU_PID" 2>/dev/null || true
    wait "$QEMU_PID" 2>/dev/null || true
  fi
}

cleanup() {
  stop_qemu
}

# Build U-Boot with the RustSBI firmware embedded as the OpenSBI payload. The
# build products are deliberately not cached: the SPL embeds the firmware, so
# they must be rebuilt from the commit under test every run.
build_uboot() {
  local tarball="${WORK_DIR}/u-boot-${UB_VERSION}.tar.gz"
  local tree="${WORK_DIR}/u-boot-${UB_VERSION}"
  local rustsbi_abs bootargs bootcmd

  # `make -C` runs inside the U-Boot tree, so OPENSBI must be an absolute
  # path; a relative path would be resolved against the tree, not the repo.
  rustsbi_abs="$(readlink -f "$RUSTSBI")"

  mkdir -p "$WORK_DIR"
  download_asset "$UB_URL" "$tarball" "$UB_SHA256"

  rm -rf "$tree"
  tar -xzf "$tarball" -C "$WORK_DIR"

  make -C "$tree" ARCH=riscv CROSS_COMPILE="$CROSS_COMPILE" \
    OPENSBI="$rustsbi_abs" qemu-riscv64_spl_defconfig

  bootargs="$(<"$CMDLINE_FILE")"
  # The command line is pasted into the boot command, parsed by U-Boot's hush
  # and rewritten into .config by scripts/config (sed), so the two characters
  # that would silently change either meaning are rejected instead.
  if [[ "$bootargs" == *"&"* || "$bootargs" == *"'"* ]]; then
    echo "Asterinas command line contains an ampersand or quote U-Boot cannot carry" >&2
    return 1
  fi

  # Set the default boot command non-interactively, equivalent to the
  # menuconfig step in the repository guide. The single quotes keep the command
  # one hush word and the ${...} references for U-Boot to expand at runtime;
  # `scripts/config` rewrites the value with sed, so it must not contain `&`.
  bootcmd="load virtio 0 \${kernel_addr_r} asterinas.uimg; load virtio 0 \${ramdisk_addr_r} initramfs.cpio.gz; setenv rd_size \${filesize}; setenv bootargs '${bootargs}'; fdt addr \${fdtcontroladdr}; fdt move \${fdtcontroladdr} \${fdt_addr_r} 0x10000; bootm \${kernel_addr_r} \${ramdisk_addr_r}:\${rd_size} \${fdt_addr_r}"
  "${tree}/scripts/config" --file "${tree}/.config" --enable USE_BOOTCOMMAND
  "${tree}/scripts/config" --file "${tree}/.config" --set-str BOOTCOMMAND "$bootcmd"

  make -C "$tree" ARCH=riscv CROSS_COMPILE="$CROSS_COMPILE" \
    OPENSBI="$rustsbi_abs" -j"$(nproc)"

  UB_TREE="$tree"
  UB_SPL="${tree}/spl/u-boot-spl"
  UB_ITB="${tree}/u-boot.itb"
  test -s "$UB_SPL" && test -s "$UB_ITB"

  # The SPL FIT must carry the firmware built from the commit under test, not a
  # stale object: dump the embedded OpenSBI image and compare it byte for byte.
  "${tree}/tools/dumpimage" -T flat_dt -p 1 -o "${WORK_DIR}/rustsbi-from-fit.bin" "$UB_ITB" \
    >"${WORK_DIR}/fit-layout.log"
  cmp "$rustsbi_abs" "${WORK_DIR}/rustsbi-from-fit.bin"
  echo "Verified u-boot.itb contains current-commit RustSBI" >&2
}

# Asterinas is a fixed-address ELF, not a Linux `Image`, so `booti` cannot load
# it. Flatten it and wrap it in a legacy uImage that tells `bootm` where to copy
# it and that it takes the Linux boot protocol. The flat binary is extended to
# the end of the last LOAD segment so the kernel's NOBITS .bss is covered:
# `bootm` copies only the bytes it is given, while QEMU's ELF loader zero-fills
# every segment's memory size.
make_kernel_uimage() {
  local base="" end="" entry size
  local type paddr memsz

  # Offset, virtual address, file size and flags are not needed here, so read
  # them into the throwaway `_` slot.
  while read -r type _ _ paddr _ memsz _; do
    [[ "$type" == LOAD ]] || continue
    paddr=$((16#${paddr#0x}))
    memsz=$((16#${memsz#0x}))
    if [[ -z "$base" || "$paddr" -lt "$base" ]]; then
      base=$paddr
    fi
    if [[ -z "$end" || $((paddr + memsz)) -gt "$end" ]]; then
      end=$((paddr + memsz))
    fi
  done < <("${CROSS_COMPILE}readelf" -lW "$KERNEL_ELF")

  entry=$("${CROSS_COMPILE}readelf" -hW "$KERNEL_ELF" | awk '/Entry point address/ { print $4 }')

  "${CROSS_COMPILE}objcopy" -O binary "$KERNEL_ELF" "$ASTERINAS_KERNEL_BIN"
  size=$(stat -c %s "$ASTERINAS_KERNEL_BIN")
  if (( size > end - base )); then
    echo "Flattened kernel is larger than its LOAD segments (${size} > $((end - base)))" >&2
    return 1
  fi
  truncate -s "$((end - base))" "$ASTERINAS_KERNEL_BIN"

  "${UB_TREE}/tools/mkimage" -A riscv -O linux -T kernel -C none \
    -a "$(printf '0x%x' "$base")" -e "$entry" -n Asterinas \
    -d "$ASTERINAS_KERNEL_BIN" "$ASTERINAS_UIMAGE" >/dev/null
  test -s "$ASTERINAS_UIMAGE"
}

# A whole-disk FAT filesystem written with mtools, so building the boot disk
# needs neither root nor loop devices. It is rebuilt every run from the freshly
# wrapped uImage.
make_boot_disk() {
  rm -f "$BOOT_DISK"
  truncate -s 64M "$BOOT_DISK"
  mkfs.vfat -F 32 "$BOOT_DISK" >/dev/null
  mcopy -i "$BOOT_DISK" "$ASTERINAS_UIMAGE" ::asterinas.uimg
  mcopy -i "$BOOT_DISK" "$INITRAMFS" ::initramfs.cpio.gz
}

check_prerequisites() {
  test -s "$RUSTSBI" || {
    echo "Missing $RUSTSBI; run 'cargo prototyper build' first" >&2
    return 1
  }
  command -v docker >/dev/null || {
    echo "docker is required to build Asterinas" >&2
    return 1
  }
  if [[ "$BOOT_MODE" = u-boot ]]; then
    command -v make mkfs.vfat mcopy \
      "${CROSS_COMPILE}gcc" "${CROSS_COMPILE}objcopy" "${CROSS_COMPILE}readelf" >/dev/null || {
      echo "The u-boot path needs make, mtools, dosfstools and the riscv64 toolchain" >&2
      return 1
    }
  fi
  qemu-system-riscv64 --version
}

start_qemu_sbi() {
  local append
  append="$(<"$CMDLINE_FILE")"

  mkdir -p "$LOG_DIR"
  # Truncate any stale log from a previous run before QEMU starts, so the
  # wait loop cannot mistake an old success marker for this boot's.
  : >"$LOG_FILE"
  qemu-system-riscv64 \
    -machine virt \
    -cpu "$ASTERINAS_CPU" \
    -smp "$ASTERINAS_SMP" \
    -m "$ASTERINAS_MEMORY" \
    -nographic \
    -no-reboot \
    -bios "$RUSTSBI" \
    -kernel "$KERNEL_ELF" \
    -initrd "$INITRAMFS" \
    -append "$append" \
    >"$LOG_FILE" 2>&1 &
  QEMU_PID=$!
}

start_qemu_uboot() {
  mkdir -p "$LOG_DIR"
  : >"$LOG_FILE"
  qemu-system-riscv64 \
    -machine virt \
    -cpu "$ASTERINAS_CPU" \
    -smp "$ASTERINAS_SMP" \
    -m "$ASTERINAS_MEMORY" \
    -nographic \
    -no-reboot \
    -bios "$UB_SPL" \
    -device loader,file="$UB_ITB",addr=0x80200000 \
    -blockdev driver=file,filename="$BOOT_DISK",node-name=hd0 \
    -device virtio-blk-device,drive=hd0 \
    >"$LOG_FILE" 2>&1 &
  QEMU_PID=$!
}

# The marker is checked together with the RustSBI banner, so the log proves the
# firmware handed off before Asterinas reached userspace. Whole-line matching
# keeps an echoed command line from satisfying the check. The u-boot path also
# proves that U-Boot ran and started the kernel.
userspace_is_ready() {
  grep -Eq $'^\r?'"${SUCCESS_MARKER}"$'\r*$' "$LOG_FILE" || return 1
  grep -Fq 'RustSBI version' "$LOG_FILE" || return 1
  if [[ "$BOOT_MODE" = u-boot ]]; then
    local marker
    for marker in "${UB_SUCCESS_MARKERS[@]}"; do
      grep -Fq "$marker" "$LOG_FILE" || return 1
    done
  fi
}

boot_has_failed() {
  if grep -Eq "$BOOT_FAILURE_PATTERN" "$LOG_FILE"; then
    return 0
  fi
  if [[ "$BOOT_MODE" = u-boot ]] && grep -Eq "$UB_BOOT_FAILURE_PATTERN" "$LOG_FILE"; then
    return 0
  fi
  return 1
}

report_boot_failure() {
  echo "Asterinas failed to boot:" >&2
  grep -E --max-count=5 "$BOOT_FAILURE_PATTERN" "$LOG_FILE" >&2 || true
  if [[ "$BOOT_MODE" = u-boot ]]; then
    grep -E --max-count=5 "$UB_BOOT_FAILURE_PATTERN" "$LOG_FILE" >&2 || true
  fi
  tail -n 120 "$LOG_FILE" || true
}

report_early_exit() {
  local qemu_exit
  set +e
  wait "$QEMU_PID"
  qemu_exit=$?
  set -e

  echo "QEMU exited before Asterinas reached userspace (exit=${qemu_exit})" >&2
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

  echo "Asterinas did not reach userspace within ${BOOT_TIMEOUT_SECS}s" >&2
  tail -n 120 "$LOG_FILE" || true
  return 1
}

main() {
  trap cleanup EXIT

  check_prerequisites
  prepare_asterinas

  if [[ "$BOOT_MODE" = u-boot ]]; then
    build_uboot
    make_kernel_uimage
    make_boot_disk
    start_qemu_uboot
  else
    start_qemu_sbi
  fi
  wait_for_userspace

  echo "RustSBI booted Asterinas ${ASTERINAS_COMMIT:0:8} to userspace successfully (${BOOT_MODE})"
  echo "QEMU log: ${LOG_FILE}"
}

main "$@"
