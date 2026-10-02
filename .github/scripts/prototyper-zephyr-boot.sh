#!/usr/bin/env bash
#
# Boot Zephyr in S-mode through RustSBI Prototyper in QEMU. The board is
# `qemu_riscv64/qemu_virt_riscv64/opensbi`, whose device tree declares S-mode, so
# RustSBI replaces QEMU's bundled OpenSBI as the M-mode layer; the sample's timer
# path runs through the SBI TIME extension and does not need the PLIC. A run
# passes when the RustSBI banner and `MetaIRQ Test Complete` both appear.
#
# Zephyr and its SDK are pinned revisions whose build products are cached;
# RustSBI is rebuilt every run and is not.

set -euo pipefail

# Zephyr commit under test, and the Zephyr SDK release that provides the
# riscv64 toolchain. Both are pinned: the tag is informational, the commit is
# what the workspace is checked out to.
readonly ZEPHYR_VERSION="v4.4.0-17679-g6607bdd711f"
readonly ZEPHYR_COMMIT="6607bdd711f49a6d1a971b3d6da2075cd918d8ed"
readonly ZEPHYR_URL="https://github.com/zephyrproject-rtos/zephyr.git"

readonly SDK_VERSION="1.0.1"
readonly SDK_TOOLCHAIN="riscv64-zephyr-elf"
readonly SDK_URL_BASE="https://github.com/zephyrproject-rtos/sdk-ng/releases/download/v${SDK_VERSION}"

readonly BOARD="qemu_riscv64/qemu_virt_riscv64/opensbi"
readonly SAMPLE="samples/kernel/metairq_dispatch"

# The firmware under test really ran. RustSBI prints this before handing over.
readonly RUSTSBI_MARKER='Hello RustSBI!'
# The sample reached the end of its run.
readonly SUCCESS_MARKER='MetaIRQ Test Complete'

# A successful run must be free of these. The first three are the shared
# boot-test failure markers (firmware/scripts/qemu-forbidden.txt); the rest are
# Zephyr and RISC-V specific.
readonly BOOT_FAILURE_PATTERN='panicked|FAILED|SystemFailure|ZEPHYR FATAL ERROR|Fatal exception|Kernel panic|not syncing|System shutdown scheduled due to RustSBI panic'

readonly CACHE_DIR="${ZEPHYR_CACHE_DIR:-.cache/zephyr}"
readonly WORK_DIR="${ZEPHYR_WORK_DIR:-.zephyr/work}"
readonly LOG_DIR="${QEMU_LOG_DIR:-qemu-logs}"
readonly RUSTSBI="${ZEPHYR_RUSTSBI:-target/riscv64gc-unknown-none-elf/release/rustsbi-prototyper-dynamic.elf}"
readonly LOG_NAME="prototyper-zephyr.log"

readonly QEMU="${ZEPHYR_QEMU:-qemu-system-riscv64}"
readonly CPU="${ZEPHYR_CPU:-rv64,s=on,u=on,pmp=on,priv_spec=v1.12.0,sv39=on}"
readonly MEMORY_MB="${ZEPHYR_MEMORY_MB:-256}"
readonly BOOT_TIMEOUT_SECS="${ZEPHYR_BOOT_TIMEOUT_SECS:-300}"

# Resolve a path that may not exist yet. GNU `realpath -m` is the common case;
# BSD realpath (macOS) lacks -m, and there the shell can resolve it directly.
abspath() {
  if realpath -m "" >/dev/null 2>&1; then
    realpath -m "$1"
  else
    case "$1" in
      /*) printf '%s\n' "$1" ;;
      *) printf '%s\n' "$PWD/$1" ;;
    esac
  fi
}

# Absolute paths for everything the build touches: `west init` changes directory
# into the workspace it creates, so relative paths would later resolve against
# the wrong place. Assign first and freeze afterwards, because
# `readonly var="$(cmd)"` would hide the command's exit status.
_abs_cache="$(abspath "$CACHE_DIR")"
_abs_work="$(abspath "$WORK_DIR")"
_abs_log_dir="$(abspath "$LOG_DIR")"
_abs_rustsbi="$(abspath "$RUSTSBI")"
readonly CACHE_DIR_ABS="$_abs_cache"
readonly WORK_DIR_ABS="$_abs_work"
readonly LOG_DIR_ABS="$_abs_log_dir"
readonly RUSTSBI_ABS="$_abs_rustsbi"
readonly WORKSPACE_ABS="${CACHE_DIR_ABS}/zephyrproject"
readonly ZEPHYR_BASE_ABS="${WORKSPACE_ABS}/zephyr"
readonly VENV_ABS="${CACHE_DIR_ABS}/venv"
readonly SDK_DIR_ABS="${CACHE_DIR_ABS}/zephyr-sdk"
readonly LOG_FILE_ABS="${LOG_DIR_ABS}/${LOG_NAME}"
readonly ZEPHYR_ELF_ABS="${WORK_DIR_ABS}/zephyr.elf"
readonly WEST="${VENV_ABS}/bin/west"
unset _abs_cache _abs_work _abs_log_dir _abs_rustsbi

# Zephyr's build helper needs its own Python dependencies. `west packages pip
# --install` pulls the full requirements set (compliance tooling, debug probes,
# emulator bindings and more); only these are needed to configure and build a
# RISC-V target, and keeping the set small keeps this job fast.
readonly PYTHON_DEPS=(jsonschema pykwalify pyelftools packaging psutil)

QEMU_PID=""

stop_qemu() {
  if [[ -n "$QEMU_PID" ]] && kill -0 "$QEMU_PID" 2>/dev/null; then
    kill "$QEMU_PID" 2>/dev/null || true
    wait "$QEMU_PID" 2>/dev/null || true
  fi
}

cleanup() {
  stop_qemu
}

check_prerequisites() {
  # The Zephyr build needs a Linux host toolchain: the SDK ships Linux host
  # tools, and the build expects GNU userland utilities. Fail early with a
  # readable message instead of part-way through the build.
  if [[ "$(uname -s)" != "Linux" ]]; then
    echo "This script requires a Linux host (CI runs on Ubuntu);" >&2
    echo "on other systems run it in the repository's development image:" >&2
    echo "  docker build -t rustsbi-dev ." >&2
    echo "  docker run --rm -v \"\$PWD:/workspace\" -w /workspace rustsbi-dev \\" >&2
    echo "    bash .github/scripts/prototyper-zephyr-boot.sh" >&2
    return 1
  fi

  local required
  for required in git python3 cmake ninja sha256sum realpath tar; do
    command -v "$required" >/dev/null || {
      echo "${required} is required to build Zephyr" >&2
      return 1
    }
  done
  test -s "$RUSTSBI_ABS" || { echo "missing RustSBI firmware: ${RUSTSBI_ABS}" >&2; return 1; }
  if ! "$QEMU" --version >/dev/null 2>&1; then
    echo "failed to run $QEMU; install QEMU (e.g. 'apt-get install qemu-system-misc')" >&2
    return 1
  fi
  "$QEMU" --version | head -n 1
}

prepare_venv() {
  if [[ ! -x "$WEST" ]]; then
    python3 -m venv "$VENV_ABS"
  fi
  "${VENV_ABS}/bin/pip" install --quiet --upgrade pip
  "${VENV_ABS}/bin/pip" install --quiet west "${PYTHON_DEPS[@]}"
  "$WEST" --version
}

prepare_workspace() {
  local revision
  revision="$(git -C "$ZEPHYR_BASE_ABS" rev-parse HEAD 2>/dev/null || true)"
  if [[ "$revision" == "$ZEPHYR_COMMIT" ]]; then
    echo "Reusing Zephyr workspace at ${ZEPHYR_BASE_ABS}"
    return
  fi

  echo "Fetching Zephyr ${ZEPHYR_VERSION}"
  if [[ ! -d "$WORKSPACE_ABS/.west" ]]; then
    rm -rf "$WORKSPACE_ABS"
    mkdir -p "$CACHE_DIR_ABS"
    "$WEST" init -m "$ZEPHYR_URL" "$WORKSPACE_ABS"
  fi

  # `west init` clones the manifest repository at its default branch, and a
  # cached workspace may sit at any revision, so pin the checkout before
  # updating the modules: the manifest at this revision decides which module
  # revisions are fetched.
  if ! git -C "$ZEPHYR_BASE_ABS" cat-file -e "${ZEPHYR_COMMIT}^{commit}" 2>/dev/null; then
    git -C "$ZEPHYR_BASE_ABS" fetch --depth 1 origin "$ZEPHYR_COMMIT"
  fi
  git -C "$ZEPHYR_BASE_ABS" checkout --detach "$ZEPHYR_COMMIT"

  # Narrow clones keep the workspace small; the build only needs the modules
  # listed in the manifest at the pinned revision.
  (cd "$WORKSPACE_ABS" && "$WEST" update --narrow -o=--depth=1)

  revision="$(git -C "$ZEPHYR_BASE_ABS" rev-parse HEAD)"
  if [[ "$revision" != "$ZEPHYR_COMMIT" ]]; then
    echo "Zephyr workspace is at ${revision}, expected ${ZEPHYR_COMMIT}" >&2
    return 1
  fi
  echo "Zephyr workspace pinned at ${revision}"
}

download_sdk_asset() {
  local asset="$1"
  local sha256="$2"
  curl --fail --location --retry 3 --retry-all-errors \
    --output "${CACHE_DIR_ABS}/${asset}" "${SDK_URL_BASE}/${asset}"
  echo "${sha256}  ${CACHE_DIR_ABS}/${asset}" | sha256sum --check
  tar xf "${CACHE_DIR_ABS}/${asset}" -C "$SDK_DIR_ABS"
  rm -f "${CACHE_DIR_ABS}/${asset}"
}

prepare_sdk() {
  local toolchain="${SDK_DIR_ABS}/zephyr-sdk-${SDK_VERSION}/gnu/${SDK_TOOLCHAIN}/bin/${SDK_TOOLCHAIN}-gcc"
  if [[ -x "$toolchain" ]]; then
    echo "Reusing Zephyr SDK at ${SDK_DIR_ABS}"
  else
    echo "Installing Zephyr SDK ${SDK_VERSION} (${SDK_TOOLCHAIN})"
    rm -rf "$SDK_DIR_ABS"
    mkdir -p "$SDK_DIR_ABS"

    local hostarch
    case "$(uname -m)" in
      x86_64 | amd64) hostarch="x86_64" ;;
      aarch64 | arm64) hostarch="aarch64" ;;
      *)
        echo "Unsupported host architecture for the Zephyr SDK: $(uname -m)" >&2
        return 1
        ;;
    esac

    # The SDK is assembled from two pinned release assets rather than
    # `west sdk install`, which resolves the release through the GitHub API and
    # fails on an unauthenticated rate limit. The minimal bundle carries the host
    # tools and the CMake package; the toolchain ships separately and must land
    # in gnu/, where the Zephyr build system looks for it. Digests come from the
    # release's sha256.sum.
    local minimal_asset minimal_sha256 toolchain_asset toolchain_sha256
    case "$hostarch" in
      x86_64)
        minimal_asset="zephyr-sdk-${SDK_VERSION}_linux-x86_64_minimal.tar.xz"
        minimal_sha256="ca9bc0ff66fafca1dac9d592a36d953cf16d096a9d09b1c0357f021cf9f6a7eb"
        toolchain_asset="toolchain_gnu_linux-x86_64_${SDK_TOOLCHAIN}.tar.xz"
        toolchain_sha256="01750834c471fbdb335c1b8b8aee17010a1968938957db85640c366235771a38"
        ;;
      aarch64)
        minimal_asset="zephyr-sdk-${SDK_VERSION}_linux-aarch64_minimal.tar.xz"
        minimal_sha256="d79c5bfc68e679488659bea289a4026e52a64f03338875c8c9c850fff13cee30"
        toolchain_asset="toolchain_gnu_linux-aarch64_${SDK_TOOLCHAIN}.tar.xz"
        toolchain_sha256="7000feff1cdcf872b88bb987696035aab0807b566e9a9842881438187df6e848"
        ;;
    esac

    download_sdk_asset "$minimal_asset" "$minimal_sha256"
    download_sdk_asset "$toolchain_asset" "$toolchain_sha256"

    # The toolchain asset unpacks to the SDK root, but the Zephyr build system
    # looks for it under gnu/.
    if [[ -d "${SDK_DIR_ABS}/${SDK_TOOLCHAIN}" && ! -e "${SDK_DIR_ABS}/zephyr-sdk-${SDK_VERSION}/gnu/${SDK_TOOLCHAIN}" ]]; then
      mkdir -p "${SDK_DIR_ABS}/zephyr-sdk-${SDK_VERSION}/gnu"
      mv "${SDK_DIR_ABS}/${SDK_TOOLCHAIN}" "${SDK_DIR_ABS}/zephyr-sdk-${SDK_VERSION}/gnu/"
    fi
  fi

  # The SDK must expose the CMake package and the toolchain in the layout the
  # Zephyr build system looks for.
  test -s "${SDK_DIR_ABS}/zephyr-sdk-${SDK_VERSION}/cmake/Zephyr-sdkConfig.cmake"
  test -x "$toolchain"
  "$toolchain" --version | head -n 1
}

build_zephyr() {
  mkdir -p "$WORK_DIR_ABS"
  (
    export ZEPHYR_BASE="${WORKSPACE_ABS}/zephyr"
    export ZEPHYR_TOOLCHAIN_VARIANT=zephyr
    export ZEPHYR_SDK_INSTALL_DIR="${SDK_DIR_ABS}"
    cd "$WORKSPACE_ABS"
    rm -rf build
    "$WEST" build -b "$BOARD" "${ZEPHYR_BASE}/${SAMPLE}" -d build
  )
  cp "${WORKSPACE_ABS}/build/zephyr/zephyr.elf" "$ZEPHYR_ELF_ABS"
  test -s "$ZEPHYR_ELF_ABS"
}

start_qemu() {
  mkdir -p "$LOG_DIR_ABS"
  "$QEMU" \
    -machine virt \
    -cpu "$CPU" \
    -m "$MEMORY_MB" \
    -smp 1 \
    -nographic \
    -no-reboot \
    -bios "$RUSTSBI_ABS" \
    -kernel "$ZEPHYR_ELF_ABS" \
    >"$LOG_FILE_ABS" 2>&1 &
  QEMU_PID=$!
}

boot_has_failed() {
  grep -Eq "$BOOT_FAILURE_PATTERN" "$LOG_FILE_ABS"
}

test_succeeded() {
  grep -Fq "$RUSTSBI_MARKER" "$LOG_FILE_ABS" \
    && grep -Fq "$SUCCESS_MARKER" "$LOG_FILE_ABS"
}

report_boot_failure() {
  echo "Boot failure detected in ${LOG_FILE_ABS}" >&2
  tail -n 120 "$LOG_FILE_ABS" || true
}

wait_for_completion() {
  local elapsed
  for ((elapsed = 0; elapsed < BOOT_TIMEOUT_SECS; elapsed++)); do
    if test_succeeded; then
      return 0
    fi
    if boot_has_failed; then
      report_boot_failure
      return 1
    fi
    if ! kill -0 "$QEMU_PID" 2>/dev/null; then
      report_exit_or_success
      return $?
    fi
    sleep 1
  done

  echo "Zephyr did not complete its test within ${BOOT_TIMEOUT_SECS}s" >&2
  tail -n 120 "$LOG_FILE_ABS" || true
  return 1
}

# QEMU may exit on its own right after printing the marker; accept that case and
# report an early exit otherwise.
report_exit_or_success() {
  set +e
  wait "$QEMU_PID"
  set -e
  if test_succeeded; then
    return 0
  fi
  echo "QEMU exited before Zephyr completed its test" >&2
  tail -n 120 "$LOG_FILE_ABS" || true
  return 1
}

main() {
  trap cleanup EXIT
  check_prerequisites
  prepare_venv
  prepare_workspace
  prepare_sdk
  build_zephyr
  start_qemu
  wait_for_completion

  echo "RustSBI booted Zephyr (${ZEPHYR_VERSION}, board ${BOARD}) successfully"
  echo "QEMU log: ${LOG_FILE_ABS}"
}

main "$@"
