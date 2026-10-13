#!/usr/bin/env bash
# Pin and boot all nine rCore-Tutorial v3 chapter kernels through current RustSBI.
# This is one sequential CI job. Guest-only compatibility shims are explicitly recorded.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/../.."

readonly REPO="${RCORE_TUTORIAL_REPO:-https://github.com/rcore-os/rCore-Tutorial-v3.git}"
readonly FIRMWARE="${RCORE_TUTORIAL_RUSTSBI:-target/riscv64gc-unknown-none-elf/release/rustsbi-prototyper-dynamic.bin}"
readonly LOG_DIR="${QEMU_LOG_DIR:-qemu-logs/rcore-tutorial}"
readonly TOOLCHAIN=1.94.0

# Serial validation and bounded QEMU execution: all in this one Shell script.
chapter_markers() {
  local ch=$1
  markers=(
    'Hello RustSBI!'
    'SiFiveClint (Base Address: 0x2000000)'
    'Redirecting hart 0 to 0x00000080200000 in Supervisor mode.'
  )
  case "$ch" in
    1) markers+=('[kernel] Hello, world!') ;;
    2) markers+=('[kernel] Hello, world!' '[kernel] num_app = 5'
                'Test power OK!' 'All applications completed!') ;;
    3|4) markers+=('[kernel]' 'Test sleep OK!' 'All applications completed!') ;;
    5) markers+=('[kernel]' 'Usertests passed!' 'Idle process exit with exit_code 0') ;;
    6) markers+=('[kernel]' 'found a block device of size'
                 'Usertests passed!' 'Idle process exit with exit_code 0') ;;
    7|8) markers+=('[kernel]' 'found a block device of size' 'Rust user shell' '>> ') ;;
    9) markers+=('CLOCK_FREQ from device tree:' 'KERN: init gpu'
                 'KERN: init keyboard' 'KERN: init mouse'
                 'found a block device of size' 'Rust user shell' '>> ') ;;
    *) echo "Unsupported chapter $ch" >&2; return 2 ;;
  esac
}

has_fatal() {
  LC_ALL=C grep -aEiq \
    'Panicked at|Kernel panic|Synchronous Exception|Unhandled exception|Usertests failed!|System shutdown scheduled due to RustSBI panic|VFS: Unable to mount root fs|U-Boot 20[0-9][0-9]|RISC-V EDK2 firmware|GNU GRUB' "$1"
}

# Check ordered offsets, not only presence of each marker.
check_serial() {
  local ch=$1 file=$2 marker found previous=-1 size
  test -s "$file" || return 1
  size="$(wc -c < "$file" | tr -d '[:space:]')"
  (( size <= 3000000 )) || return 1
  has_fatal "$file" && return 1
  chapter_markers "$ch" || return 1
  for marker in "${markers[@]}"; do
    found="$(LC_ALL=C grep -aFbo -m 1 -- "$marker" "$file" | sed -n '1s/:.*//p' || true)"
    [[ "$found" =~ ^[0-9]+$ ]] || return 1
    (( found > previous )) || return 1
    previous=$found
  done
}

# Exactly nine positive fixtures and eight negative cases for each chapter.
self_test() {
  local ch fixture good bad cases=0 last n
  fixture="$(mktemp "${TMPDIR:-/tmp}/rcore-serial-test.XXXXXX")"
  for ch in {1..9}; do
    chapter_markers "$ch"
    good="$(printf '%s\n' "${markers[@]}")"
    printf '%s\n' "$good" > "$fixture"
    check_serial "$ch" "$fixture" || { echo "baseline ch$ch failed" >&2; rm -f "$fixture"; return 1; }
    (( cases += 1 ))
    local -a invalid=(
      "${good/Hello RustSBI!/Unknown firmware}"
      "${good/SiFiveClint (Base Address: 0x2000000)/IMSIC only}"
      "${good/Supervisor mode./Machine mode.}"
      "$(printf '%s\n' "$good" | sed '$s/.*/MISSING_END/')"
      "$good"$'\nPanicked at src/trap/mod.rs:151'
      "$good"$'\nUsertests failed!'
      "$good"$'\nU-Boot 2025.10'
    )
    last=$((${#markers[@]} - 1))
    n=$((${#markers[@]} - 2))
    bad="$(printf '%s\n' "${markers[@]:0:$n}" "${markers[$last]}" "${markers[$n]}")"
    invalid+=("$bad")
    for bad in "${invalid[@]}"; do
      printf '%s\n' "$bad" > "$fixture"
      if check_serial "$ch" "$fixture"; then
        echo "false-positive ch$ch" >&2
        rm -f "$fixture"
        return 1
      fi
      (( cases += 1 ))
    done
  done
  rm -f "$fixture"
  echo "PASS: $cases deterministic rCore-Tutorial serial checks"
}

# ch1-6 must exit cleanly. Interactive ch7-9 must be live 2.5 seconds after
# shell-ready evidence, without any fatal logs. Never accept a mere firmware banner.
boot_chapter() {
  local ch=$1 kernel=$2 fs=$3 serial="$LOG_DIR/ch$1-serial.log"
  local timeout="${RCORE_TUTORIAL_BOOT_TIMEOUT:-75}"
  if ! [[ "$timeout" =~ ^[0-9]+$ ]] || (( timeout < 15 || timeout > 300 )); then
    echo "Invalid boot timeout" >&2
    return 2
  fi
  local -a cmd=(
    qemu-system-riscv64 -machine "virt,acpi=off" -smp 1 -m 128M
    -display none -serial stdio -monitor none -no-reboot
    -bios "$FIRMWARE" -kernel "$kernel"
  )
  if (( ch < 9 )); then cmd+=(-net none); fi
  if (( ch >= 6 )); then
    test -s "$fs" || return 1
    local bus=0
    (( ch == 9 )) && bus=7
    cmd+=(-drive "file=$fs,if=none,format=raw,id=x0"
          -device "virtio-blk-device,drive=x0,bus=virtio-mmio-bus.$bus")
  fi
  if (( ch == 9 )); then
    cmd+=(-device 'virtio-gpu-device,bus=virtio-mmio-bus.6'
          -device 'virtio-keyboard-device,bus=virtio-mmio-bus.4'
          -device 'virtio-mouse-device,bus=virtio-mmio-bus.5'
          -netdev 'user,id=net0'
          -device 'virtio-net-device,netdev=net0,bus=virtio-mmio-bus.3')
  fi
  printf '%q ' "${cmd[@]}" > "$LOG_DIR/ch$ch-qemu-command.txt"
  printf '\n' >> "$LOG_DIR/ch$ch-qemu-command.txt"
  : > "$serial"

  local pid rc=0 tick=0 ready_at=-1 intentional=0 failure=''
  local max_ticks=$((timeout * 4))
  "${cmd[@]}" > "$serial" 2>&1 < /dev/null &
  pid=$!
  for ((tick=0; tick<max_ticks; tick++)); do
    sleep 0.25
    if has_fatal "$serial"; then failure='fatal UART evidence'; break; fi
    if (( $(wc -c < "$serial") > 3000000 )); then
      failure='serial output exceeded 3 MB'
      break
    fi
    if (( ch >= 7 )) && check_serial "$ch" "$serial"; then
      if (( ready_at < 0 )); then ready_at=$tick; fi
      if (( tick - ready_at >= 10 )) && kill -0 "$pid" 2>/dev/null; then
        intentional=1
        break
      fi
    fi
    if ! kill -0 "$pid" 2>/dev/null; then break; fi
  done
  if kill -0 "$pid" 2>/dev/null; then kill -TERM "$pid" 2>/dev/null || true; fi
  wait "$pid" || rc=$?
  if [[ -z "$failure" ]] && (( tick >= max_ticks )); then failure='boot timeout'; fi
  if [[ -z "$failure" ]] && ! check_serial "$ch" "$serial"; then
    failure='missing, unordered, or fatal UART evidence'
  fi
  if [[ -z "$failure" ]] && (( ch >= 7 && intentional != 1 )); then
    failure='shell not live for the settling interval'
  fi
  if [[ -z "$failure" ]] && (( ch <= 6 && rc != 0 )); then
    failure="QEMU exited with $rc"
  fi
  if [[ -n "$failure" ]]; then
    echo "ch$ch: FAIL ($failure) QEMU exit=$rc UART=$serial" >&2
    tail -c 2500 "$serial" >&2
    return 1
  fi
  echo "ch$ch: PASS (chapter $ch guest milestone reached); QEMU exit=$rc; UART=$serial"
}

# Snapshot each actual, non-dev upstream chapter, not a mutable branch name.
readonly -a REVISIONS=(
  unused
  85fe952a04c4b671fef1beb2f470591ae7ea5123
  fc4af3367b7feebb2287b15fe3ff23757471e571
  9ca4aedb66f626dcc72f835a5946742a82b61c7c
  f384222b9b5bf33c4eb6b1a87ad5a7a2b8122c79
  a55bd5598a19cd5878af9bd8211540cff48dd4d1
  47bbec297476f0009b8b3ac99fc9fb11743bd837
  af89affce453082c4aacd646b9bd908eba5cee79
  3b1bc659ffdbbe0dcf457ddd81ed45261bf3dc75
  209476f27359b47cd6534ebb91afc5d01692b429
)

if [[ ${1:-} == --self-test ]]; then
  test $# -eq 1
  self_test
  exit 0
fi
if (( $# == 0 )); then
  CHAPTERS=(1 2 3 4 5 6 7 8 9)
else
  CHAPTERS=("$@")
  for chapter in "${CHAPTERS[@]}"; do
    case "$chapter" in 1|2|3|4|5|6|7|8|9) ;; *)
      echo "Usage: $0 [--self-test|1 ... 9]" >&2; exit 2 ;;
    esac
  done
fi
test -s "$FIRMWARE"
mkdir -p "$LOG_DIR"
WORK="$(mktemp -d "${TMPDIR:-/tmp}/rustsbi-rcore-tutorial.XXXXXX")"
readonly WORK
cleanup() { rm -rf -- "$WORK"; }
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

readonly SOURCE_REPO="$WORK/repo"
git init --quiet "$SOURCE_REPO"
git -C "$SOURCE_REPO" remote add origin "$REPO"
export RUSTUP_TOOLCHAIN="$TOOLCHAIN"
echo "rCore-Tutorial: one CI item, chapters ${CHAPTERS[*]}, direct SBI only"
echo "Rust toolchain: $(rustc --version); QEMU: $(qemu-system-riscv64 --version | head -1)"

for chapter in "${CHAPTERS[@]}"; do
  sha="${REVISIONS[$chapter]}"
  dir="$WORK/ch$chapter"
  mkdir -p "$dir"
  echo "=== ch$chapter @ $sha ==="

  # Fetch exact commit and unpack tracked source. No edits to the repository.
  git -C "$SOURCE_REPO" fetch --quiet --depth=1 origin "$sha"
  test "$(git -C "$SOURCE_REPO" rev-parse FETCH_HEAD)" = "$sha"
  git -C "$SOURCE_REPO" archive FETCH_HEAD | tar -xf - -C "$dir"
  test -f "$dir/os/Cargo.toml"

  shim=none
  if (( chapter <= 8 )); then
    # These upstream chapters use SBI Legacy console putchar/getchar; current
    # Prototyper does not implement the legacy console extension. This is a
    # TEST-GUEST-ONLY adaptation. Its original user applications are unchanged.
    python3 - "$dir/os/src/sbi.rs" "$chapter" <<'PY_SHIM'
from pathlib import Path
import sys
p = Path(sys.argv[1])
s = p.read_text()
old_out = '    #[allow(deprecated)]\n    sbi_rt::legacy::console_putchar(c);'
new_out = '''    // Test-guest shim: SBI DBCN write_byte EID 0x4442434E / FID 2.
    unsafe {
        core::arch::asm!(
            "ecall",
            inlateout("a0") c => _,
            lateout("a1") _,
            in("a6") 2usize,
            in("a7") 0x4442434Eusize,
        );
    }'''
old_in = '    #[allow(deprecated)]\n    sbi_rt::legacy::console_getchar()'
new_in = '''    // Test-guest shim: poll the standard QEMU NS16550 UART (no SBI DBCN read).
    // A zero byte signals no data, as required by the tutorial stdin driver.
    unsafe {
        if core::ptr::read_volatile(0x1000_0005 as *const u8) & 1 == 0 { 0 }
        else { core::ptr::read_volatile(0x1000_0000 as *const u8) as usize }
    }'''
if s.count(old_out) != 1:
    raise SystemExit('Pinned chapter does not contain the expected legacy putchar')
s = s.replace(old_out, new_out)
if old_in in s:
    if s.count(old_in) != 1: raise SystemExit('Ambiguous legacy getchar')
    s = s.replace(old_in, new_in)
p.write_text(s)
# ch7/ch8 use the Legacy getchar after enabling paging, but their board MMIO
# map lacks the physical UART page. Add only that mapping in the disposable
# test guest so the explicit UART stdin adapter is safe in S-mode.
if int(sys.argv[2]) in (7, 8):
    board = p.parent / "boards/qemu.rs"
    source = board.read_text()
    anchor = "    (0x1000_1000, 0x00_1000), // Virtio Block in virt machine"
    if source.count(anchor) != 1:
        raise SystemExit("Expected tutorial virtio MMIO board definition missing")
    board.write_text(source.replace(
        anchor,
        "    (0x1000_0000, 0x00_1000), // CI guest UART input adapter MMIO\n" + anchor,
    ))
print('Adapted guest Legacy console output/input; guest_unmodified=false')
PY_SHIM
    shim=legacy-to-dbcn-and-uart
  fi
  # ch5/ch6 have finite, self-checking usertests. The later chapters contain
  # long or interactive tests; accept only a live user-shell milestone there.
  test_mode=1
  if (( chapter >= 7 )); then test_mode=0; fi

  if [[ -d "$dir/user" ]]; then
    make -C "$dir/user" build "TEST=$test_mode"
  fi
  cp "$dir/os/src/linker-qemu.ld" "$dir/os/src/linker.ld"
  (cd "$dir/os" && cargo build --release)
  kernel="$dir/os/target/riscv64gc-unknown-none-elf/release/os.bin"
  rust-objcopy "$dir/os/target/riscv64gc-unknown-none-elf/release/os" --strip-all -O binary "$kernel"
  test -s "$kernel"

  if (( chapter >= 6 )); then
    # The generated fs.img belongs only to this disposable checkout and may
    # be written by user tests. Never mount or alter an original guest image.
    make -C "$dir/os" fs-img "TEST=$test_mode"
    fs="$dir/user/target/riscv64gc-unknown-none-elf/release/fs.img"
    test -s "$fs"
  fi

  {
    printf 'source=rcore-os/rCore-Tutorial-v3\nsource_sha=%s\nchapter=%s\n' "$sha" "$chapter"
    printf 'firmware=RustSBI-from-checked-out-revision\nguest_unmodified=%s\n' "$([[ "$shim" == none ]] && echo true || echo false)"
    printf 'guest_console_adapter=%s\ntest_mode=%s\n' "$shim" "$test_mode"
    printf 'rust_toolchain=%s\nqemu=%s\n' "$TOOLCHAIN" "$(qemu-system-riscv64 --version | head -1)"
    shasum -a 256 "$FIRMWARE" "$kernel"
    for lock in "$dir/os/Cargo.lock" "$dir/user/Cargo.lock" "$dir/easy-fs-fuse/Cargo.lock"; do
      if test -f "$lock"; then shasum -a 256 "$lock"; fi
    done
    if (( chapter >= 6 )); then shasum -a 256 "$fs"; fi
  } > "$LOG_DIR/ch$chapter-provenance.log"

  boot_chapter "$chapter" "$kernel" "${fs:-}"

done

echo 'PASS: all requested rCore-Tutorial chapters booted through direct RustSBI'
