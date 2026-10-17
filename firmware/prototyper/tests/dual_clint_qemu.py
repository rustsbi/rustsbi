#!/usr/bin/env python3
"""QEMU NUMA regression with two actual emulated CLINT devices.

From RustSBI root:
    cargo prototyper test --no-run
    cargo prototyper build
    python3 firmware/prototyper/tests/dual_clint_qemu.py

The stock TIME suite tests hart 0. This regression exercises HSM,
RFENCE and cross-socket IPI for harts 2/3, not their local TIME IRQ.
"""
import argparse
import re
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
FIRMWARE = ROOT / "target/riscv64gc-unknown-none-elf/release/rustsbi-prototyper-dynamic.elf"
KERNEL = ROOT / "target/riscv64imac-unknown-none-elf/release/rustsbi-test-kernel.bin"
ANSI = re.compile(r"\x1b\[[0-9;]*m")


def run(firmware: Path, kernel: Path, timeout: int) -> None:
    for path in (firmware, kernel):
        if not path.is_file():
            raise SystemExit(f"Missing: {path}. Build firmware and test kernel first.")
    cmd = [
        "qemu-system-riscv64", "-machine", "virt",
        "-smp", "4,sockets=2,cores=2,threads=1", "-m", "256M",
        "-object", "memory-backend-ram,id=ram0,size=128M",
        "-object", "memory-backend-ram,id=ram1,size=128M",
        "-numa", "node,nodeid=0,memdev=ram0,cpus=0-1",
        "-numa", "node,nodeid=1,memdev=ram1,cpus=2-3",
        "-nographic", "-bios", str(firmware), "-kernel", str(kernel)
    ]
    try:
        result = subprocess.run(
            cmd, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
            timeout=timeout, check=False
        )
    except subprocess.TimeoutExpired as exc:
        output = ANSI.sub("", (exc.stdout or b"").decode("utf-8", "replace"))
        raise SystemExit(f"FAIL QEMU timeout after {timeout}s:\n{output[-3000:]}")
    output = ANSI.sub("", result.stdout.decode("utf-8", "replace"))
    expected = [
        "Hello RustSBI!",
        "Platform HART Count           : 4",
        "SiFiveClint (Base Address: 0x2000000, harts: [0, 1])",
        "SiFiveClint (Base Address: 0x2010000, harts: [2, 3])",
        "Sbi `TIME` test pass",
        "Sbi `sPI` test pass",
        "remote RFence to started hart 2 pass",
        "remote RFence to started hart 3 pass",
        "hart 2 resumed",
        "hart 3 resumed",
        "Sbi stopped-hart IPI/RFENCE test pass: hart 2",
        "Sbi stopped-hart IPI/RFENCE test pass: hart 3",
        "SBI tests completed: PASS",
    ]
    missing = [part for part in expected if part not in output]
    if result.returncode != 0 or missing:
        raise SystemExit(
            f"FAIL exit={result.returncode} missing={missing}\n{output[-4000:]}"
        )
    print("PASS: QEMU 2 NUMA sockets / 2 actual CLINT devices")
    print("PASS: hart 2/3 HSM, RFENCE, cross-socket IPI/resume/stop")
    print("PASS: QEMU exit 0 and SBI tests completed: PASS")
    print("NOTE: stock TIME suite only tests boot hart 0")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--firmware", type=Path, default=FIRMWARE)
    parser.add_argument("--kernel", type=Path, default=KERNEL)
    parser.add_argument("--timeout", type=int, default=30)
    args = parser.parse_args()
    run(args.firmware, args.kernel, args.timeout)
