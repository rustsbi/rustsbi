#!/usr/bin/env bash
# SPDX-License-Identifier: MulanPSL-2.0

set -euo pipefail
source "$(dirname -- "${BASH_SOURCE[0]}")/common.sh"
parse_options "$@"
select_qemu

blocked=0
fixable=0
host_prerequisites || blocked=1
if (( BUILD_QEMU )); then
    printf 'FIX   qemu source build required (%s)\n' "$QEMU_WANT"
    fixable=1
else
    printf 'PASS  qemu %s\n' "$QEMU_BIN"
fi

# Rust proxies can download a missing toolchain even for --version.
# Inspect their paths only; toolchain synchronization belongs to setup.
for tool in rustup cargo rust-objcopy; do
    if command -v "$tool" >/dev/null; then
        printf 'PASS  %s present (not executed)\n' "$tool"
    elif [[ -x $CARGO_DIR/bin/$tool || -x $TOOLS_ROOT/bin/$tool ]]; then
        printf 'PASS  %s present outside PATH (not executed)\n' "$tool"
    else
        printf 'FIX   %s missing\n' "$tool"
        fixable=1
    fi
done
printf 'INFO  toolchain repository-pinned toolchain/components are checked during approved setup\n'
for tool in mkimage dtc; do
    command -v "$tool" >/dev/null || printf 'WARN  %s optional; required for packed images/device trees\n' "$tool"
done
write_inventory
if (( blocked )); then
    printf 'VERDICT: BLOCKED (native prerequisites must be prepared)\n'
    exit 3
elif (( fixable )); then
    printf 'VERDICT: FIXABLE (review the write inventory before setup)\n'
    exit 2
fi
printf 'VERDICT: READY (host tools present; repository toolchain not executed)\n'
