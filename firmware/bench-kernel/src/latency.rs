//! Warm, batched SBI round-trip measurements; console output is outside timing.

use core::arch::asm;
use sbi_spec::binary::SbiRet;

const CALLS: usize = 4096;
const SAMPLES: usize = 9;

#[inline(always)]
fn call(eid: usize, fid: usize, args: [usize; 3]) -> SbiRet {
    let error;
    let value;
    // SBI preserves all registers except a0/a1. Memory effects of the ecall
    // deliberately remain visible to the compiler around the measurements.
    unsafe {
        asm!("ecall", inlateout("a0") args[0] => error,
             inlateout("a1") args[1] => value, in("a2") args[2],
             in("a3") 0usize, in("a4") 0usize, in("a5") 0usize,
             in("a6") fid, in("a7") eid, options(nostack));
    }
    SbiRet { error, value }
}

fn counters() -> (usize, usize, usize) {
    let (cycles, instructions, ticks);
    // Complete preceding memory/I/O before sampling. The fence and reads
    // are paid once per batch, rather than once per SBI call.
    unsafe {
        asm!("fence iorw, iorw", "rdcycle {cycles}", "rdinstret {instructions}",
             "rdtime {ticks}", cycles = out(reg) cycles,
             instructions = out(reg) instructions, ticks = out(reg) ticks,
             options(nostack));
    }
    (cycles, instructions, ticks)
}

fn measure(name: &str, eid: usize, fid: usize, args: [usize; 3], expected_error: usize) {
    for _ in 0..64 {
        assert_eq!(call(eid, fid, args).error, expected_error);
    }
    let mut cycles = [0; SAMPLES];
    let mut instructions = [0; SAMPLES];
    let mut ticks = [0; SAMPLES];
    for sample in 0..SAMPLES {
        let mut errors = 0;
        let before = counters();
        for _ in 0..CALLS {
            errors |= call(eid, fid, args).error ^ expected_error;
        }
        let after = counters();
        assert_eq!(errors, 0, "unexpected SBI result in {name}");
        cycles[sample] = after.0.wrapping_sub(before.0);
        instructions[sample] = after.1.wrapping_sub(before.1);
        ticks[sample] = after.2.wrapping_sub(before.2);
    }
    cycles.sort_unstable();
    instructions.sort_unstable();
    ticks.sort_unstable();
    println!(
        "BENCH {name} calls={CALLS} samples={SAMPLES} cycles_min={} cycles_med={} instret_med={} ticks_med={}",
        cycles[0],
        cycles[SAMPLES / 2],
        instructions[SAMPLES / 2],
        ticks[SAMPLES / 2],
    );
}

pub(super) fn run(hart_id: usize, timebase: u32) {
    println!("SBI latency: totals per batch, timebase={timebase} Hz");
    measure("base_spec", 0x10, 0, [0; 3], 0);
    measure("base_probe", 0x10, 3, [0x48534d, 0, 0], 0);
    measure("base_mvendorid", 0x10, 4, [0; 3], 0);
    measure(
        "unsupported",
        0x7fff_ffff,
        0,
        [0; 3],
        SbiRet::not_supported().error,
    );
    for (name, eid, fid, args) in [
        ("hsm_status", 0x48534d, 2, [hart_id, 0, 0]),
        ("pmu_num", 0x504d55, 0, [0; 3]),
        ("time_set", 0x54494d45, 0, [usize::MAX, usize::MAX, 0]),
        ("ipi_empty", 0x735049, 0, [0; 3]),
        ("rfence_empty", 0x52464e43, 1, [0; 3]),
        ("rfence_self", 0x52464e43, 1, [1, hart_id, 0]),
        (
            "dbcn_empty",
            0x4442434e,
            0,
            [0, b"SBI".as_ptr() as usize, 0],
        ),
    ] {
        if call(0x10, 3, [eid, 0, 0]).value != 0 {
            measure(name, eid, fid, args, 0);
        } else {
            println!("BENCH {name} unavailable");
        }
    }
    println!("SBI latency completed: PASS");
}
