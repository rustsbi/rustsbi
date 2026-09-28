#!/usr/bin/env bash
# Build assets with: nix build path:./.github/nixos -o target/nixos-boot-assets
# Build firmware with: cargo prototyper build
set -euo pipefail

ASSETS=${NIXOS_BOOT_ASSETS:-target/nixos-boot-assets}
RUSTSBI=${NIXOS_RUSTSBI:-target/riscv64gc-unknown-none-elf/release/rustsbi-prototyper-dynamic.elf}
LOG_DIR=${QEMU_LOG_DIR:-qemu-logs}
BOOT_TIMEOUT=${NIXOS_BOOT_TIMEOUT_SECS:-300}
SMP=${NIXOS_SMP:-2}
QEMU=${QEMU:-qemu-system-riscv64}
LOG_FILE="${LOG_DIR}/prototyper-nixos-smp${SMP}.log"

for input in "$RUSTSBI" "$ASSETS/Image" "$ASSETS/initrd" "$ASSETS/cmdline" "$ASSETS/SHA256SUMS"; do
  if [[ ! -s "$input" ]]; then
    echo "Missing boot input: $input" >&2
    exit 1
  fi
done
(cd "$ASSETS" && sha256sum --check SHA256SUMS)
mkdir -p "$LOG_DIR"

status=0
timeout --kill-after=10s "${BOOT_TIMEOUT}s" "$QEMU" \
  -machine virt -accel tcg -m 2G -smp "$SMP" \
  -nographic -monitor none -nic none -no-reboot \
  -bios "$RUSTSBI" \
  -kernel "$ASSETS/Image" \
  -initrd "$ASSETS/initrd" \
  -append "$(cat "$ASSETS/cmdline")" \
  >"$LOG_FILE" 2>&1 || status=$?

if [[ $status -ne 0 ]]; then
  echo "NixOS QEMU failed or timed out (exit $status); see $LOG_FILE" >&2
  tail -n 100 "$LOG_FILE"
  exit 1
fi
if grep -Eq 'Kernel panic|not syncing|Attempted to kill init|System shutdown scheduled due to RustSBI panic' "$LOG_FILE"; then
  echo "Boot failure detected; see $LOG_FILE" >&2
  tail -n 100 "$LOG_FILE"
  exit 1
fi
if ! grep -Fq 'RustSBI' "$LOG_FILE" || ! grep -Eq '^RUSTSBI-NIXOS-BOOT-OK version=' "$LOG_FILE"; then
  echo "RustSBI banner or NixOS stage-2 success marker missing; see $LOG_FILE" >&2
  tail -n 100 "$LOG_FILE"
  exit 1
fi
grep -E '^RUSTSBI-NIXOS-BOOT-OK version=' "$LOG_FILE"
echo "RustSBI booted NixOS and powered off successfully (smp=$SMP); log: $LOG_FILE"
