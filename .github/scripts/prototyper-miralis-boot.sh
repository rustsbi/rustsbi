#!/usr/bin/env bash
# Boot the Miralis-relocated Prototyper mbench payload image under Miralis.
set -euo pipefail

target_dir=${CARGO_TARGET_DIR:-target}
miralis=${MIRALIS_IMAGE:-miralis/target/riscv-unknown-miralis/debug/miralis.img}
firmware=${MIRALIS_FIRMWARE:-$target_dir/riscv64gc-unknown-none-elf/release/rustsbi-prototyper-payload-test.bin}
forbidden=firmware/scripts/qemu-forbidden.txt
log=qemu-logs/prototyper-miralis-test.log
trace=qemu-logs/prototyper-miralis-traps.log
timeout_secs=${MIRALIS_BOOT_TIMEOUT_SECS:-120}

for image in "$miralis" "$firmware" "$forbidden"; do
  test -s "$image" || { echo "missing or empty input: $image" >&2; exit 1; }
done
mkdir -p qemu-logs

set +e
# Pinned Miralis does not handle virtual-instruction exceptions from H guests.
# Native Prototyper QEMU tests cover HS/VS trap routing with H enabled.
timeout "${timeout_secs}s" qemu-system-riscv64 \
  -machine virt -cpu rv64,h=false -m 256M -smp 1 -nographic -no-reboot -bios "$miralis" \
  -device "loader,file=$firmware,addr=0x80200000,force-raw=on" \
  -d int,guest_errors -D "$trace" \
  </dev/null >"$log" 2>&1
status=$?
set -e

if (( status != 0 )); then
  echo "QEMU exited with status $status; last 120 log lines:" >&2
  tail -n 120 "$log" >&2
  echo "Last trapped instructions:" >&2
  tail -n 20 "$trace" >&2
  exit 1
fi

verify_log() {
  if [[ ! -s "$log" ]]; then
    echo "QEMU produced empty serial log: $log" >&2
    return 1
  fi

  local forbidden_file=${forbidden:-firmware/scripts/qemu-forbidden.txt}
  if [[ ! -s "$forbidden_file" ]]; then
    echo "missing or empty forbidden pattern file: $forbidden_file" >&2
    return 1
  fi

  local pattern
  while IFS= read -r pattern || [[ -n "$pattern" ]]; do
    pattern="${pattern#"${pattern%%[![:space:]]*}"}"
    pattern="${pattern%"${pattern##*[![:space:]]}"}"
    [[ -z "$pattern" || "$pattern" == \#* ]] && continue
    if grep -Fq -- "$pattern" "$log"; then
      echo "serial log contains forbidden pattern: $pattern" >&2
      return 1
    fi
  done < "$forbidden_file"

  local mbench_target_dir="${CARGO_TARGET_DIR:-target}/mbench-cargo"
  local mbench_bin="$mbench_target_dir/release/mbench"
  env -u CARGO_ENCODED_RUSTFLAGS -u RUSTFLAGS cargo build --locked \
    --manifest-path firmware/mbench/Cargo.toml \
    --target-dir "$mbench_target_dir" \
    --package mbench-cli --bin mbench --release
  if [[ ! -x "$mbench_bin" ]]; then
    echo "missing mbench CLI executable: $mbench_bin" >&2
    return 1
  fi

  local report_json validate_status=0
  set +e
  report_json=$("$mbench_bin" validate "$log" --json)
  validate_status=$?
  set -e

  if [[ -z "$report_json" ]]; then
    echo "mbench validate produced no JSON report (exit $validate_status)" >&2
    return 1
  fi

  python3 - "$validate_status" 3<<<"$report_json" <<'PY'
import json
import os
import sys

validate_status = int(sys.argv[1])
with os.fdopen(3, encoding="utf-8") as report_stream:
    report = json.load(report_stream)

errors = list(report.get("protocol_errors") or [])
if validate_status != 0 or not report.get("protocol_valid") or not report.get("completed"):
    detail = "; ".join(errors) if errors else f"validate exit={validate_status}"
    sys.exit(f"mbench protocol audit failed: {detail}")

if report.get("panic") is not None:
    sys.exit(f"mbench target panicked: {report['panic']!r}")

meta = report.get("meta")
if not isinstance(meta, dict):
    sys.exit("mbench report missing meta frame")
if meta.get("hart_count") != 1:
    sys.exit(f"expected hart_count=1, got {meta.get('hart_count')!r}")
cfg = meta.get("config") or {}
if cfg.get("mode") != "check":
    sys.exit(f"expected config.mode='check', got {cfg.get('mode')!r}")

counts = report.get("counts") or {}
for field in (
    "checks_failed",
    "check_assertions_failed",
    "benches_failed",
    "quarantine_warnings",
):
    if counts.get(field) != 0:
        sys.exit(f"{field}={counts.get(field)!r}")

checks = report.get("checks") or []
benches = report.get("benches") or []
for item in checks + benches:
    if item.get("status") == "FAIL":
        sys.exit(f"mbench item failed: {item.get('id')!r}")

required_exts = {
    "base", "time", "spi", "rfnc", "hsm", "srst", "pmu",
    "dbcn", "susp", "cppc", "sta", "fwft", "dbtr", "mpxy",
}
present_exts = set(meta.get("extensions") or [])
missing_exts = sorted(required_exts - present_exts)
if missing_exts:
    sys.exit(f"missing expected SBI extensions on QEMU virt: {', '.join(missing_exts)}")

required_checks = [
    "conformance/base",
    "conformance/time",
    "conformance/spi",
    "conformance/rfnc",
    "conformance/pmu",
    "conformance/dbcn",
    "conformance/fwft",
    "conformance/dbtr",
    "conformance/shmem_security.sta",
    "conformance/shmem_security.dbtr",
    "conformance/shmem_security.mpxy",
    "conformance/system",
    "conformance/traps",
]
check_status = {c.get("id"): c.get("status") for c in checks}
for check_id in required_checks:
    status = check_status.get(check_id)
    if status != "PASS":
        sys.exit(f"required check {check_id!r} status is {status!r}, expected 'PASS'")
PY
}

if ! verify_log; then
  echo "Verification failed; last 120 log lines:" >&2
  tail -n 120 "$log" >&2
  exit 1
fi

echo "Miralis -> Prototyper -> mbench boot passed; log: $log"
