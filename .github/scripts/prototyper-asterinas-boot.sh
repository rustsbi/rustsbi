#!/usr/bin/env bash
#
# Boot Asterinas through RustSBI Prototyper in QEMU.
#
# RustSBI's dynamic firmware follows the fw_dynamic handoff convention, so QEMU
# starts the kernel directly: `-bios` points at the firmware and `-kernel` at
# the Asterinas ELF, with QEMU passing the device tree and entry point to the
# firmware through the dynamic info structure. Asterinas reads its command line
# from `/chosen/bootargs` and its initramfs from `/chosen/linux,initrd-*`, so
# the bare path needs no intermediate bootloader.
#
# Asterinas publishes no prebuilt riscv64 boot artifacts, so it is built from a
# pinned commit inside its official development image. Only the build products
# (the kernel ELF and the bundle initramfs) and the resulting command line are
# cached, so a warm run skips the container build entirely; the image digest and
# the source commit are both checked before the build starts, and RustSBI is
# rebuilt from the commit under test and is never cached.
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
# (to run the Asterinas build image), git and qemu-system-riscv64.

set -euo pipefail

if (( $# > 0 )); then
  echo "Usage: $0" >&2
  exit 2
fi

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

readonly RUSTSBI="${ASTERINAS_RUSTSBI:-target/riscv64gc-unknown-none-elf/release/rustsbi-prototyper-dynamic.elf}"

readonly CACHE_DIR="${ASTERINAS_CACHE_DIR:-.cache/asterinas}"
readonly WORK_DIR="${ASTERINAS_WORK_DIR:-.asterinas/work}"
readonly LOG_DIR="${QEMU_LOG_DIR:-qemu-logs}"
readonly LOG_FILE="${LOG_DIR}/prototyper-asterinas.log"

readonly BOOT_TIMEOUT_SECS="${ASTERINAS_BOOT_TIMEOUT_SECS:-300}"

# Asterinas uses Svpbmt for page-table PBMT bits and reads `seed` under Zkr;
# both are what its own qemu_args.sh selects for riscv64.
readonly ASTERINAS_CPU="${ASTERINAS_CPU:-rv64,svpbmt=true,zkr=true}"
readonly ASTERINAS_MEMORY="${ASTERINAS_MEMORY:-2G}"
readonly ASTERINAS_SMP="${ASTERINAS_SMP:-1}"

readonly KERNEL_ELF="${CACHE_DIR}/aster-nix-boot.elf"
readonly INITRAMFS="${CACHE_DIR}/initramfs-boot.cpio.gz"
readonly CMDLINE_FILE="${CACHE_DIR}/cmdline.txt"

# The marker is printed by `/test/boot_hello.sh`, which the OSDK bundle runs as
# the init command when AUTO_TEST=boot. Matching the whole line keeps the same
# text echoed on a command line from satisfying the check. Asterinas runs the
# init command through `script`, so the line carries carriage returns on both
# ends; allow any number of them.
readonly SUCCESS_MARKER='Successfully booted\.'
# A panic halts the machine while QEMU stays alive, so without this the job
# would sit out the whole timeout and report only that the marker never
# appeared. The last two patterns are Asterinas's own wording.
readonly BOOT_FAILURE_PATTERN="Uncaught panic|Cannot handle kernel CPU exception|Kernel panic|not syncing|Attempted to kill init|SBI panic"

QEMU_PID=""

check_prerequisites() {
  test -s "$RUSTSBI" || {
    echo "Missing $RUSTSBI; run 'cargo prototyper build' first" >&2
    return 1
  }
  command -v docker >/dev/null || {
    echo "docker is required to build Asterinas" >&2
    return 1
  }
  qemu-system-riscv64 --version
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

start_qemu() {
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

# The marker is checked together with the RustSBI banner, so the log proves the
# firmware handed off before Asterinas reached userspace. Whole-line matching
# keeps an echoed command line from satisfying the check.
userspace_is_ready() {
  grep -Eq $'^\r?'"${SUCCESS_MARKER}"$'\r*$' "$LOG_FILE" &&
    grep -Fq 'RustSBI version' "$LOG_FILE"
}

boot_has_failed() {
  grep -Eq "$BOOT_FAILURE_PATTERN" "$LOG_FILE"
}

report_boot_failure() {
  echo "Asterinas failed to boot:" >&2
  grep -E --max-count=5 "$BOOT_FAILURE_PATTERN" "$LOG_FILE" >&2 || true
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
  start_qemu
  wait_for_userspace

  echo "RustSBI booted Asterinas ${ASTERINAS_COMMIT:0:8} to userspace successfully"
  echo "QEMU log: ${LOG_FILE}"
}

main "$@"
