#!/usr/bin/env bash
# RustSBI -> Salus -> Buildroot Linux, or Tellus -> confidential guest.
set -Eeuo pipefail

readonly CACHE_VERSION=1

# Fixed inputs for the dependency bundle.
readonly SALUS_COMMIT=27b8d1dbeec96cbcac929c7875a21ec0af03d1bb
readonly SALUS_SBI_RS_COMMIT=10247f521a6070eb65c6fc2f52b8dc8b5fb121ca
readonly SALUS_RICE_COMMIT=67c3a1c80b673710192da58a477f606276d8467c
readonly QEMU_VERSION=10.0.14
readonly QEMU_SHA256=c7230968906fdb5540d803e2af1f515e77590a414f2a4dbd3ec9ada3c47083d0
readonly LINUX_VERSION=6.12
readonly LINUX_SHA256=b1a2562be56e42afb3f8489d4c2a7ac472ac23098f1ef1c1e40da601f54625eb
readonly BUILDROOT_COMMIT=e3959a039059d6b3bc95100409975e87e9c14d3b
readonly BUILDROOT_SHA256=04b0a7c0532d8266853a03e578eae9d30d43f6183d94fd5b2d9b69b36d554e21
readonly BAZELISK_VERSION=1.26.0
readonly BAZELISK_SHA256=6539c12842ad76966f3d493e8f80d67caa84ec4a000e220d5459833c967c12bc
readonly BAZEL_VERSION=6.0.0

recipe_fingerprint() {
  (
    cd "$REPO_ROOT" || exit 1
    declare -p CACHE_VERSION SALUS_COMMIT SALUS_SBI_RS_COMMIT SALUS_RICE_COMMIT \
      QEMU_VERSION QEMU_SHA256 LINUX_VERSION LINUX_SHA256 \
      BUILDROOT_COMMIT BUILDROOT_SHA256 BAZELISK_VERSION BAZELISK_SHA256 BAZEL_VERSION \
      PRODUCT_FILES || exit 1
    declare -f download extract apply_patch_once prepare_qemu prepare_salus \
      prepare_linux prepare_buildroot || exit 1
    sha256sum \
      .github/salus/buildroot_defconfig .github/salus/rootfs-overlay/etc/init.d/S99rustsbi-smoke \
      .github/salus/patches/*.patch || exit 1
    uname -sm || exit 1
    gcc --version | sed -n '1p' || exit 1
    g++ --version | sed -n '1p' || exit 1
    riscv64-linux-gnu-gcc --version | sed -n '1p' || exit 1
    python3 --version || exit 1
    ldd --version | sed -n '1p' || exit 1
    pkg-config --modversion glib-2.0 pixman-1 slirp || exit 1
  ) | sha256sum | cut -d ' ' -f 1
}

verify_products() {
  local manifest_files
  [[ -f "$PRODUCTS/SHA256SUMS" ]] || return 1
  manifest_files="$(awk 'NF != 2 {exit 1} {print $2}' "$PRODUCTS/SHA256SUMS" | LC_ALL=C sort)" || return 1
  [[ "$manifest_files" == "$(printf '%s\n' "${PRODUCT_FILES[@]}" | LC_ALL=C sort)" ]] || return 1
  (cd "$PRODUCTS" && sha256sum --check --status SHA256SUMS)
}

download() {
  local url=$1 destination=$2 digest=$3 temp
  if [[ -s "$destination" ]] && printf '%s  %s\n' "$digest" "$destination" | sha256sum --check --status; then
    return
  fi
  temp=$(mktemp "${destination}.part.XXXXXX")
  if ! curl -4 --fail --location --connect-timeout 30 --max-time "$DOWNLOAD_TIMEOUT" \
    --retry 3 --retry-all-errors --output "$temp" "$url"; then
    rm -f "$temp"
    return 1
  fi
  if ! printf '%s  %s\n' "$digest" "$temp" | sha256sum --check; then
    rm -f "$temp"
    return 1
  fi
  mv "$temp" "$destination"
}

extract() {
  local archive=$1 directory=$2
  if [[ ! -f "$directory/.extracted" ]]; then
    mkdir -p "$directory"
    tar -xf "$archive" --strip-components=1 -C "$directory"
    touch "$directory/.extracted"
  fi
}

apply_patch_once() {
  local directory=$1 patch_file=$2
  if patch --batch --fuzz=0 --directory="$directory" --strip=1 --forward --dry-run <"$patch_file"; then
    patch --batch --fuzz=0 --directory="$directory" --strip=1 --forward <"$patch_file"
  else
    # Accept a previously applied patch only if it reverses cleanly.
    patch --batch --fuzz=0 --directory="$directory" --strip=1 --reverse --dry-run <"$patch_file"
  fi
}

prepare_qemu() {
  local src="$BUILD/src/qemu" out="$BUILD/qemu"
  download "https://download.qemu.org/qemu-$QEMU_VERSION.tar.xz" \
    "$DOWNLOADS/qemu-$QEMU_VERSION.tar.xz" "$QEMU_SHA256"
  extract "$DOWNLOADS/qemu-$QEMU_VERSION.tar.xz" "$src"
  mkdir -p "$out"
  (
    cd "$out"
    if [[ ! -f build.ninja ]]; then
      "$src/configure" --target-list=riscv64-softmmu --without-default-features \
        --enable-slirp --disable-werror --disable-docs
    fi
    ninja -j"$JOBS" qemu-system-riscv64
  )
  cp "$out/qemu-system-riscv64" "$PRODUCTS/qemu-system-riscv64"
  cp "$src/pc-bios/efi-e1000e.rom" "$PRODUCTS/efi-e1000e.rom"
}

prepare_salus() {
  local src="$BUILD/src/salus" bazelisk="$DOWNLOADS/bazelisk-linux-amd64"
  download "https://github.com/bazelbuild/bazelisk/releases/download/v$BAZELISK_VERSION/bazelisk-linux-amd64" \
    "$bazelisk" "$BAZELISK_SHA256"
  chmod u+x "$bazelisk"
  if [[ ! -d "$src/.git" ]]; then
    git init --quiet "$src"
    git -C "$src" remote add origin https://github.com/rivosinc/salus.git
  fi
  if ! git -C "$src" cat-file -e "$SALUS_COMMIT^{commit}" 2>/dev/null; then
    timeout --kill-after=5 "$DOWNLOAD_TIMEOUT" git -C "$src" \
      -c http.lowSpeedLimit=1024 -c http.lowSpeedTime=60 fetch --depth=1 origin "$SALUS_COMMIT"
  fi
  if [[ "$(git -C "$src" rev-parse --verify HEAD 2>/dev/null || true)" != "$SALUS_COMMIT" ]]; then
    git -C "$src" checkout --detach "$SALUS_COMMIT"
  fi
  timeout --kill-after=5 "$DOWNLOAD_TIMEOUT" git -C "$src" \
    -c http.lowSpeedLimit=1024 -c http.lowSpeedTime=60 submodule update --init --depth=1
  [[ "$(git -C "$src/sbi-rs" rev-parse HEAD)" == "$SALUS_SBI_RS_COMMIT" ]]
  [[ "$(git -C "$src/rice" rev-parse HEAD)" == "$SALUS_RICE_COMMIT" ]]
  # Salus's pre-UART console needs DBCN when running on RustSBI.
  apply_patch_once "$src" "$INPUT_DIR/patches/0001-salus-early-dbcn-console.patch"
  (
    cd "$src"
    export USE_BAZEL_VERSION="$BAZEL_VERSION" BAZELISK_HOME="$CACHE_DIR/bazelisk"
    "$bazelisk" --output_user_root="$BUILD/bazel" --host_jvm_args=-XX:ActiveProcessorCount="$JOBS" \
      --host_jvm_args=-Xmx2048m --host_jvm_args=-Djava.net.preferIPv4Stack=true \
      --batch build //:salus-all --//:enable_hardware_ad_updates=false \
      --jobs="$JOBS" --local_cpu_resources="$JOBS" --local_ram_resources=2048 \
      --repository_cache="$CACHE_DIR/bazel-repository-cache" --color=no --curses=no
  )
  cp "$src/bazel-bin/salus" "$PRODUCTS/salus"
  cp "$src/bazel-bin/test-workloads/tellus_guestvm" "$PRODUCTS/tellus_guestvm"
}

prepare_linux() {
  local src="$BUILD/src/linux" out="$BUILD/linux"
  download "https://cdn.kernel.org/pub/linux/kernel/v6.x/linux-$LINUX_VERSION.tar.xz" \
    "$DOWNLOADS/linux-$LINUX_VERSION.tar.xz" "$LINUX_SHA256"
  extract "$DOWNLOADS/linux-$LINUX_VERSION.tar.xz" "$src"
  make -C "$src" O="$out" ARCH=riscv CROSS_COMPILE=riscv64-linux-gnu- defconfig
  "$src/scripts/config" --file "$out/.config" --disable DEBUG_INFO \
    --enable NONPORTABLE --enable HVC_RISCV_SBI --enable BLK_DEV_NVME
  make -C "$src" O="$out" ARCH=riscv CROSS_COMPILE=riscv64-linux-gnu- olddefconfig
  make -C "$src" O="$out" ARCH=riscv CROSS_COMPILE=riscv64-linux-gnu- -j"$JOBS" Image
  cp "$out/arch/riscv/boot/Image" "$PRODUCTS/Image"
  cp "$out/.config" "$LOG_DIR/salus-linux.config"
}

prepare_buildroot() {
  local src="$BUILD/src/buildroot" out="$BUILD/buildroot" overlay="$BUILD/rootfs-overlay"
  download "https://codeload.github.com/buildroot/buildroot/tar.gz/$BUILDROOT_COMMIT" \
    "$DOWNLOADS/buildroot-$BUILDROOT_COMMIT.tar.gz" "$BUILDROOT_SHA256"
  extract "$DOWNLOADS/buildroot-$BUILDROOT_COMMIT.tar.gz" "$src"
  mkdir -p "$out" "$overlay"
  cp -a "$INPUT_DIR/rootfs-overlay/." "$overlay/"
  chmod 755 "$overlay/etc/init.d/S99rustsbi-smoke"
  cp "$INPUT_DIR/buildroot_defconfig" "$out/salus_defconfig"
  printf 'BR2_ROOTFS_OVERLAY="%s"\n' "$overlay" >>"$out/salus_defconfig"
  # Buildroot's host configure scripts otherwise refuse builds as root.
  if ((EUID == 0)); then export FORCE_UNSAFE_CONFIGURE=1; fi
  make -C "$src" O="$out" BR2_DEFCONFIG="$out/salus_defconfig" defconfig
  make -C "$src" O="$out" -j"$JOBS" BR2_DL_DIR="$DOWNLOADS/buildroot-packages" \
    BR2_PRIMARY_SITE=https://sources.buildroot.net BR2_BACKUP_SITE=https://sources.buildroot.net \
    'BR2_WGET=wget -4 --passive-ftp -nd -t 3 --timeout=30'
  cp --sparse=always "$out/images/rootfs.ext2" "$PRODUCTS/rootfs.ext2"
  cp "$out/.config" "$LOG_DIR/salus-buildroot.config"
}

prepare_dependencies() {
  [[ "$JOBS" =~ ^[1-9][0-9]*$ ]] || {
    echo 'Invalid SALUS_BUILD_JOBS' >&2
    exit 2
  }
  [[ "$DOWNLOAD_TIMEOUT" =~ ^[1-9][0-9]*$ ]] || {
    echo 'Invalid SALUS_DOWNLOAD_TIMEOUT_SECS' >&2
    exit 2
  }
  [[ "$(uname -sm)" == 'Linux x86_64' ]] || {
    echo 'Requires Linux x86_64' >&2
    exit 2
  }
  for tool in gcc g++ riscv64-linux-gnu-gcc make ninja curl wget git patch \
    python3 tar xz bzip2 rsync cpio bc bison flex pkg-config flock; do
    command -v "$tool" >/dev/null || {
      echo "Missing build tool: $tool" >&2
      exit 1
    }
  done
  mkdir -p "$CACHE_DIR" "$WORK_DIR" "$LOG_DIR" "$DOWNLOADS"
  # Serialize writers to the dependency bundle; an incomplete build has no stamp.
  exec 9>"$CACHE_DIR/.prepare.lock"
  flock 9
  RECIPE="$(recipe_fingerprint)"
  readonly RECIPE
  if [[ -f "$PRODUCTS/recipe" && "$(cat "$PRODUCTS/recipe")" == "$RECIPE" ]] &&
    verify_products; then
    echo "Using verified Salus dependency bundle ($RECIPE)"
    exec 9>&-
    return
  fi

  readonly BUILD="$WORK_DIR/dependencies-$RECIPE"
  mkdir -p "$BUILD/src" "$PRODUCTS"
  rm -f "$PRODUCTS/recipe"
  CURRENT_LOG=""
  report_failure() {
    local status=$?
    trap - ERR
    echo "Salus dependency build failed (exit=$status): $CURRENT_LOG" >&2
    if [[ -f "$CURRENT_LOG" ]]; then tail -n 80 "$CURRENT_LOG" >&2; fi
    exit "$status"
  }
  trap report_failure ERR

  for component in qemu salus linux buildroot; do
    CURRENT_LOG="$LOG_DIR/salus-prepare-$component.log"
    echo "Preparing $component (log: $CURRENT_LOG)"
    (
      trap - ERR
      "prepare_$component"
    ) >"$CURRENT_LOG" 2>&1
  done
  (
    cd "$PRODUCTS"
    sha256sum "${PRODUCT_FILES[@]}" >SHA256SUMS
  )
  printf '%s\n' "$RECIPE" >"$PRODUCTS/recipe"
  echo "Prepared Salus dependency bundle ($RECIPE)"
  trap - ERR
  exec 9>&-
}

if (($# > 1)); then
  echo "Usage: $0 [all|tellus|buildroot|prepare|fingerprint]" >&2
  exit 2
fi
readonly SCENARIO="${1:-all}"
case "$SCENARIO" in all | tellus | buildroot | prepare | fingerprint) ;; *)
  echo 'Unknown boot scenario' >&2
  exit 2
  ;;
esac
REPO_ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)"
readonly REPO_ROOT
CACHE_DIR="$(realpath -m "${SALUS_CACHE_DIR:-$REPO_ROOT/target/prototyper-salus/cache}")"
WORK_DIR="$(realpath -m "${SALUS_WORK_DIR:-$REPO_ROOT/target/prototyper-salus/work}")"
LOG_DIR="$(realpath -m "${QEMU_LOG_DIR:-$REPO_ROOT/qemu-logs}")"
readonly CACHE_DIR WORK_DIR LOG_DIR
export SALUS_CACHE_DIR="$CACHE_DIR" SALUS_WORK_DIR="$WORK_DIR" QEMU_LOG_DIR="$LOG_DIR"
readonly PRODUCTS="$CACHE_DIR/products"
readonly INPUT_DIR="$REPO_ROOT/.github/salus"
readonly JOBS="${SALUS_BUILD_JOBS:-2}"
readonly DOWNLOAD_TIMEOUT="${SALUS_DOWNLOAD_TIMEOUT_SECS:-1800}"
readonly DOWNLOADS="$CACHE_DIR/downloads"
readonly PRODUCT_FILES=(qemu-system-riscv64 efi-e1000e.rom salus tellus_guestvm Image rootfs.ext2)
if [[ "$SCENARIO" == fingerprint ]]; then
  recipe_fingerprint
  exit 0
fi
readonly CPUS="${SALUS_CPUS:-2}"
readonly BOOT_TIMEOUT="${SALUS_BOOT_TIMEOUT_SECS:-180}"
case "$CPUS" in 2 | 4) ;; *)
  echo 'SALUS_CPUS must be 2 or 4' >&2
  exit 2
  ;;
esac
[[ "$BOOT_TIMEOUT" =~ ^[1-9][0-9]*$ ]] || {
  echo 'Invalid SALUS_BOOT_TIMEOUT_SECS' >&2
  exit 2
}
mkdir -p "$WORK_DIR" "$LOG_DIR"
command -v fdtput >/dev/null || {
  echo 'Missing device-tree-compiler (fdtput)' >&2
  exit 1
}
cd "$REPO_ROOT"
prepare_dependencies
if [[ "$SCENARIO" == prepare ]]; then exit 0; fi

# Always build the checkout under test, including on a dependency cache hit.
cargo prototyper build --features hypervisor
RUSTSBI="$(realpath "${CARGO_TARGET_DIR:-$REPO_ROOT/target}/riscv64gc-unknown-none-elf/release/rustsbi-prototyper-dynamic.elf")"
readonly RUSTSBI
test -s "$RUSTSBI"
QEMU_PID=""
stop_qemu() {
  if [[ -n "$QEMU_PID" ]] && kill -0 "$QEMU_PID" 2>/dev/null; then
    # timeout forwards TERM to QEMU and enforces its --kill-after deadline.
    kill -TERM "$QEMU_PID" 2>/dev/null || true
    wait "$QEMU_PID" 2>/dev/null || true
  fi
}
trap stop_qemu EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

check_boot_log() {
  local scenario=$1 log=$2 status=$3 text pattern failed=0
  text="$(sed -E 's/\x1b\[[0-?]*[ -/]*[@-~]//g;s/\r//g' "$log")" || return 1
  if ((status != 0)); then
    echo "QEMU exit status $status (124 means timeout)" >&2
    failed=1
  fi
  if grep -Eiq '^FAIL:|\bpanic(ked)?\b|No IOMMU found!|IOMMU probe failure|Error during initialization|falling back to CLINT|Unhandled (exception|trap)|Unexpected (exception|trap|guest interrupt|host VM trap)' <<<"$text"; then
    echo "Fatal event in $log" >&2
    failed=1
  fi
  local -a required=(
    'Hello RustSBI!'
    "^Brought online $CPUS CPU\(s\)$"
    '^IMSIC at 0x[0-9a-f]+; 5 guest interrupt files supported$'
    'RISC-V IOMMU version 0x[0-9a-f]+'
    '^Host VM requested shutdown$'
    '^Shutting down$'
  )
  case "$scenario" in
    buildroot) required+=('^RUSTSBI-SALUS-LINUX-OK$') ;;
    tellus)
      required+=(
        '^PASS: guestvm boot 0$' '^PASS: test attestation$'
        '^PASS: Test memory sharing$' '^PASS: test vectors$'
        '^PASS: test emulated mmio$' '^PASS: test interrupts$'
        '^PASS: test huge pages$' '^Tellus - All OK$'
      )
      ;;
  esac
  for pattern in "${required[@]}"; do
    if ! grep -Eq "$pattern" <<<"$text"; then
      echo "Missing boot event: $pattern" >&2
      failed=1
    fi
  done
  return "$failed"
}

boot() {
  local scenario=$1 image log prefix qemu_status dtb
  prefix="$LOG_DIR/prototyper-salus-$scenario-${CPUS}cpu"
  log="$prefix.log"
  local -a args=(
    "$PRODUCTS/qemu-system-riscv64"
    -L "$PRODUCTS"
    -M 'virt,aia=aplic-imsic,aia-guests=5'
    -cpu 'rv64,v=on,vlen=256,elen=64,smaia=true,ssaia=true,sscofpmf=true,smstateen=true'
    -smp "$CPUS" -m 2048
    -display none -serial stdio -monitor none -no-reboot
    -bios "$RUSTSBI" -kernel "$PRODUCTS/salus"
  )
  dtb="$prefix.dtb"
  # Dump before the PCI IOMMU adds its own node during device realization.
  timeout --foreground --kill-after=5 20 "${args[@]}" -machine "dumpdtb=$dtb" >"$prefix.dtb.log" 2>&1
  # guest-loader writes a 64-bit address and size; declare their cell counts.
  fdtput -t x "$dtb" /chosen '#address-cells' 2
  fdtput -t x "$dtb" /chosen '#size-cells' 2
  args+=(-dtb "$dtb" -device 'riscv-iommu-pci,vendor-id=0x1efd,device-id=0xedf1')
  case "$scenario" in
    tellus)
      image="$PRODUCTS/tellus_guestvm"
      ;;
    buildroot)
      image="$PRODUCTS/Image"
      # Never let a VM modify the cached rootfs. No loop mount or host root needed.
      mkdir -p "$WORK_DIR/run-buildroot-${CPUS}cpu"
      cp --reflink=auto --sparse=always "$PRODUCTS/rootfs.ext2" \
        "$WORK_DIR/run-buildroot-${CPUS}cpu/rootfs.ext2"
      args+=(
        -append 'console=hvc0 earlycon=sbi root=/dev/nvme0n1'
        -drive "file=$WORK_DIR/run-buildroot-${CPUS}cpu/rootfs.ext2,format=raw,id=hd,if=none"
        -device 'nvme,serial=deadbeef,drive=hd'
        -netdev 'user,id=usernet' -device 'e1000e,netdev=usernet'
      )
      ;;
  esac
  args+=(-device "guest-loader,kernel=$image,addr=0xc0200000")
  {
    printf 'RustSBI checkout: '
    git describe --always --dirty
    sha256sum "$RUSTSBI" "$dtb"
    declare -p SALUS_COMMIT SALUS_SBI_RS_COMMIT SALUS_RICE_COMMIT \
      QEMU_VERSION QEMU_SHA256 LINUX_VERSION LINUX_SHA256 \
      BUILDROOT_COMMIT BUILDROOT_SHA256 BAZELISK_VERSION BAZELISK_SHA256 BAZEL_VERSION
    cat "$PRODUCTS/recipe" "$PRODUCTS/SHA256SUMS"
    "$PRODUCTS/qemu-system-riscv64" --version
    printf 'Command:'
    printf ' %q' "${args[@]}"
    printf '\n'
  } >"$prefix.metadata"
  echo "Booting $scenario ($CPUS CPUs, log: $log)"
  timeout --foreground --kill-after=5 "$BOOT_TIMEOUT" "${args[@]}" >"$log" 2>&1 &
  QEMU_PID=$!
  if wait "$QEMU_PID"; then qemu_status=0; else qemu_status=$?; fi
  QEMU_PID=""
  printf '%s\n' "$qemu_status" >"$prefix.exit"
  if ! check_boot_log "$scenario" "$log" "$qemu_status"; then
    tail -n 120 "$log" >&2
    return 1
  fi
  echo "RustSBI -> Salus -> $scenario: passed ($CPUS CPUs)"
}

if [[ "$SCENARIO" == all ]]; then
  boot tellus
  boot buildroot
else
  boot "$SCENARIO"
fi
