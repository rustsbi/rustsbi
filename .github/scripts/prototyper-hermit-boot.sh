#!/usr/bin/env bash
# Build the Hermit loader and application from pinned commits, then boot the
# application through the loader in QEMU. Build firmware with: cargo prototyper build
set -euo pipefail

readonly HERMIT_LOADER_COMMIT="3ec52a9c585e66c0a8c7d5841293b3be28bae462"
readonly HERMIT_LOADER_URL="https://github.com/hermit-os/loader.git"
readonly HERMIT_TEMPLATE_COMMIT="69b048652d7f9d73d7fbf586d11b8ad30fc40bd4"
readonly HERMIT_TEMPLATE_URL="https://github.com/hermit-os/hermit-rs-template.git"
# The kernel repository pins this nightly for its own builds; the riscv64
# Hermit target is tier-3 and needs rust-src for -Z build-std.
readonly HERMIT_NIGHTLY="nightly-2026-09-01"
readonly HERMIT_TARGET="riscv64gc-unknown-hermit"
readonly HERMIT_LOADER_STABLE="1.98.1"
readonly LOADER_TREE=".hermit-boot/work/hermit-loader"
readonly LOADER_ELF="${LOADER_TREE}/target/release/hermit-loader-riscv64-sbi"
readonly LOADER_BIN="${LOADER_TREE}/target/release/hermit-loader-riscv64-sbi.bin"
readonly TEMPLATE_TREE=".hermit-boot/work/hermit-rs-template"
readonly HERMIT_APP="${TEMPLATE_TREE}/target/${HERMIT_TARGET}/release/hermit-rs-template"
readonly RUSTSBI="target/riscv64gc-unknown-none-elf/release/rustsbi-prototyper-dynamic.elf"
readonly LOG_DIR="${QEMU_LOG_DIR:-qemu-logs}"
readonly LOG_FILE="${LOG_DIR}/prototyper-hermit.log"
readonly BOOT_TIMEOUT_SECS="${HERMIT_BOOT_TIMEOUT_SECS:-120}"
readonly SMOKE_MARKER="Hello, world!"
# A panic halts the machine while QEMU stays alive, so a boot can fail long
# before the timeout is over.
readonly BOOT_FAILURE_PATTERN='panicked at|\[PANIC\]'

QEMU_PID=""

# Clones `url` at pinned `commit` into `tree`, reusing the tree across runs.
# A tree left broken by an interrupted earlier run is discarded and recloned;
# a complete checkout at a different commit is a moved pin and fails loudly.
clone_pinned() {
  local url=$1
  local commit=$2
  local tree=$3

  if [[ -d "$tree/.git" ]] && ! git -C "$tree" rev-parse --verify --quiet HEAD >/dev/null; then
    echo "Discarding broken checkout in ${tree}" >&2
    rm -rf "$tree"
  fi
  if [[ ! -d "$tree/.git" ]]; then
    git init --quiet "$tree"
    git -C "$tree" remote add origin "$url"
    git -C "$tree" fetch --quiet --depth=1 origin "$commit"
    git -C "$tree" checkout --quiet --detach FETCH_HEAD
  fi
  local head
  head=$(git -C "$tree" rev-parse HEAD)
  if [[ "$head" != "$commit" ]]; then
    echo "Checkout mismatch in ${tree}: expected ${commit}, got ${head}" >&2
    return 1
  fi
}

prepare_loader() {
  clone_pinned "$HERMIT_LOADER_URL" "$HERMIT_LOADER_COMMIT" "$LOADER_TREE"

  # Build with a pinned stable toolchain chosen by the loader's own CI; only
  # install when missing, so reruns never touch the network.
  (
    cd "$LOADER_TREE"
    rustup toolchain list | grep -q "^${HERMIT_LOADER_STABLE}-" \
      || rustup toolchain install "$HERMIT_LOADER_STABLE" --profile minimal \
        --target riscv64imac-unknown-none-elf >/dev/null
    RUSTUP_TOOLCHAIN="$HERMIT_LOADER_STABLE" cargo xtask build --target riscv64-sbi --release
  )

  # Flatten the loader so QEMU records `.init` (0x80200000) as the fw_dynamic
  # entry instead of the ELF image base, which is a data region.
  test -s "$LOADER_ELF"
  rust-objcopy -O binary "$LOADER_ELF" "$LOADER_BIN"
  test -s "$LOADER_BIN"
}

prepare_app() {
  clone_pinned "$HERMIT_TEMPLATE_URL" "$HERMIT_TEMPLATE_COMMIT" "$TEMPLATE_TREE"

  # The template is cloned inside this repository, whose root manifest is a
  # cargo workspace; an empty [workspace] table detaches the template from it.
  if ! grep -q '^\[workspace\]' "$TEMPLATE_TREE/Cargo.toml"; then
    printf '\n[workspace]\n' >>"$TEMPLATE_TREE/Cargo.toml"
  fi

  # Build the application, which links the Hermit kernel beneath it. The
  # nightly is pinned by this script, not by the template: its stable
  # toolchain cannot build the tier-3 riscv64 target. RUSTUP_TOOLCHAIN
  # overrides the template's own rust-toolchain.toml. Only install when
  # missing, so reruns never touch the network.
  if ! rustup toolchain list | grep -q "^${HERMIT_NIGHTLY}-"; then
    rustup toolchain install "$HERMIT_NIGHTLY" --profile minimal \
      --component rust-src >/dev/null
  fi
  rustup component add rust-src --toolchain "$HERMIT_NIGHTLY" 2>/dev/null

  (
    cd "$TEMPLATE_TREE"
    RUSTUP_TOOLCHAIN="$HERMIT_NIGHTLY" cargo build --release --locked \
      --target "$HERMIT_TARGET" -Z build-std=std,alloc
  )
  test -s "$HERMIT_APP"
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

start_qemu() {
  mkdir -p "$LOG_DIR"
  # Truncate any stale log from a previous run before QEMU starts, so the
  # wait loop cannot mistake an old success marker for this boot's.
  : >"$LOG_FILE"
  qemu-system-riscv64 \
    -machine virt \
    -nographic \
    -no-reboot \
    -smp 1 \
    -m 128M \
    -cpu rv64 \
    -bios "$RUSTSBI" \
    -kernel "$LOADER_BIN" \
    -initrd "$HERMIT_APP" \
    </dev/null >"$LOG_FILE" 2>&1 &
  QEMU_PID=$!
}

qemu_is_running() {
  if kill -0 "$QEMU_PID" 2>/dev/null; then
    return 0
  fi

  local qemu_exit
  set +e
  wait "$QEMU_PID"
  qemu_exit=$?
  set -e
  echo "QEMU exited before Hermit reached userspace (exit=${qemu_exit})" >&2
  return 1
}

boot_failed() {
  grep -Eq "$BOOT_FAILURE_PATTERN" "$LOG_FILE"
}

userspace_is_ready() {
  grep -Fq "$SMOKE_MARKER" "$LOG_FILE"
}

wait_for_hermit() {
  local tick
  for ((tick = 0; tick < BOOT_TIMEOUT_SECS * 10; tick++)); do
    if ! qemu_is_running; then
      # QEMU may have printed the marker and powered off since our last read.
      userspace_is_ready && return 0
      return 1
    fi
    if boot_failed; then
      echo "Hermit reported a fatal boot error" >&2
      return 1
    fi
    if userspace_is_ready; then
      return 0
    fi
    sleep 0.1
  done

  echo "Hermit did not reach userspace within ${BOOT_TIMEOUT_SECS}s" >&2
  return 1
}

main() {
  trap cleanup EXIT
  test -s "$RUSTSBI"
  qemu-system-riscv64 --version
  prepare_loader
  prepare_app
  start_qemu
  if ! wait_for_hermit; then
    tail -n 120 "$LOG_FILE" || true
    return 1
  fi

  echo "RustSBI booted Hermit to userspace successfully"
  echo "QEMU log: ${LOG_FILE}"
}

main "$@"
