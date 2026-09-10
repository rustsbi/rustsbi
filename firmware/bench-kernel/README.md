# SBI benchmarks

Run the firmware and benchmark kernel on QEMU with:

```sh
cargo prototyper bench --smp 8 --timeout 40
```

The kernel first measures SBI round trips on the boot hart, before starting
secondary harts. Each case warms up with 64 calls, then measures nine batches of
4096 calls using `cycle`, `instret` and `time`. Every call's error code is checked;
console output is outside the measured batches. Optional extensions are probed
and skipped when unavailable.

`BENCH` lines report batch totals. Divide `cycles_med` and `instret_med` by
`calls` for the median cost per call. Convert `ticks_med` to time using the
printed device-tree timebase frequency. These numbers include the loop and error
aggregation overhead. The minimum and median help identify noise; compare the
same kernel binary, clock configuration and firmware build options. QEMU counters
are useful for functional checks, not hardware performance claims.

The `ipi_empty`, `rfence_empty` and `dbcn_empty` cases use empty requests to isolate
entry and validation costs. `rfence_self` selects the calling hart and completes
a local TLB flush through SBI. These cases do not measure cross-hart interrupt
delivery, remote completion or UART throughput. The multicore IPI/RFence tests run afterwards;
`SBI latency completed: PASS` and `SBI benchmark completed: PASS` distinguish
completion of the two stages.

Output uses SBI DBCN, so no board-specific UART layout is needed. Hardware must
allow S-mode access to the cycle, instruction and time counters. To build a kernel
for a different physical load address, set the hexadecimal link address:

```sh
RUSTSBI_BENCH_LINK_ADDRESS=0x200000 cargo build --release \
    --target riscv64imac-unknown-none-elf -p rustsbi-bench-kernel
rust-objcopy -O binary \
    target/riscv64imac-unknown-none-elf/release/rustsbi-bench-kernel bench.bin
```

The default address is `0x80200000`. Configure the loader and firmware to enter
the image at its chosen load address and pass the hart ID and DTB address. The
image also reserves about 65 MiB for stacks; include its BSS in the memory plan.
On platforms without SBI reset support, the kernel parks after completion.
