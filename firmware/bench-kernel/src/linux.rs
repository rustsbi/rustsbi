//! SBI workloads used by Linux and supplementary interface measurements.

use core::{
    arch::{asm, naked_asm},
    sync::atomic::{AtomicUsize, Ordering},
};
use sbi_spec::{base, binary::SbiRet, dbcn, fwft, hsm, pmu, rfnc, spi as ipi, sta};

const SAMPLES: usize = 9;
const BATCH: usize = 256;
const READY: usize = 1;
const STOP: usize = 2;
const RETENTIVE: usize = 3;
const NON_RETENTIVE: usize = 4;
static COMMAND: AtomicUsize = AtomicUsize::new(0);
static ONLINE: AtomicUsize = AtomicUsize::new(0);
static ACK: AtomicUsize = AtomicUsize::new(0);
static ERROR: AtomicUsize = AtomicUsize::new(0);
static TIMEOUT_TICKS: AtomicUsize = AtomicUsize::new(0);

#[inline(always)]
fn call(eid: usize, fid: usize, args: [usize; 6]) -> SbiRet {
    let (error, value);
    // SBI preserves all registers other than a0/a1. Do not hide memory effects.
    unsafe {
        asm!("ecall", inlateout("a0") args[0] => error,
            inlateout("a1") args[1] => value, in("a2") args[2],
            in("a3") args[3], in("a4") args[4], in("a5") args[5],
            in("a6") fid, in("a7") eid, options(nostack));
    }
    SbiRet { error, value }
}

fn checked(ret: SbiRet) -> usize {
    assert_eq!(ret.error, 0, "Linux workload SBI result: {ret:?}");
    ret.value
}

fn available(eid: usize) -> bool {
    checked(call(
        base::EID_BASE,
        base::PROBE_EXTENSION,
        [eid, 0, 0, 0, 0, 0],
    )) != 0
}

fn counters() -> [usize; 3] {
    let (cycles, instructions, ticks);
    unsafe {
        asm!("fence iorw, iorw", "rdcycle {c}", "rdinstret {i}", "rdtime {t}",
            c = out(reg) cycles, i = out(reg) instructions, t = out(reg) ticks,
            options(nostack));
    }
    [cycles, instructions, ticks]
}

/// Preparation and cleanup are outside each timed batch. Stateful calls use
/// iterations=1; their result includes one counter-sampling interval per call.
fn measure(
    name: &str,
    unit: &str,
    iterations: usize,
    mut prepare: impl FnMut(),
    mut operation: impl FnMut() -> SbiRet,
    mut cleanup: impl FnMut(SbiRet),
) {
    for _ in 0..8 {
        prepare();
        let ret = operation();
        cleanup(ret);
        checked(ret);
    }
    let mut totals = [[0; SAMPLES]; 3];
    // Index the sample across three counter arrays, then sort each separately.
    #[allow(clippy::needless_range_loop)]
    for sample in 0..SAMPLES {
        prepare();
        let mut errors = 0;
        let mut last = SbiRet::success(0);
        let before = counters();
        for _ in 0..iterations {
            last = operation();
            errors |= last.error;
        }
        let after = counters();
        cleanup(last);
        assert_eq!(errors, 0, "Linux workload {name}");
        for index in 0..3 {
            totals[index][sample] = after[index].wrapping_sub(before[index]);
        }
    }
    for values in &mut totals {
        values.sort_unstable();
    }
    println!(
        "LINUX_BENCH {name} unit={unit} iterations={iterations} samples={SAMPLES} cycles_min={} cycles_med={} instret_med={} ticks_med={}",
        totals[0][0],
        totals[0][SAMPLES / 2],
        totals[1][SAMPLES / 2],
        totals[2][SAMPLES / 2]
    );
}

fn scalar(name: &str, eid: usize, fid: usize, args: [usize; 6]) {
    if !available(eid) {
        println!("LINUX_SKIP {name} reason=extension-unavailable");
        return;
    }
    let probe = call(eid, fid, args);
    if probe.error == SbiRet::not_supported().error {
        println!("LINUX_SKIP {name} reason=function-unavailable");
        return;
    }
    checked(probe);
    measure(name, "call", BATCH, || {}, || call(eid, fid, args), |_| {});
}

fn pmu_calls() {
    if !available(pmu::EID_PMU) {
        println!("LINUX_SKIP pmu reason=extension-unavailable");
        return;
    }
    scalar(
        "pmu_counter_info",
        pmu::EID_PMU,
        pmu::COUNTER_GET_INFO,
        [0; 6],
    );
    let count = checked(call(pmu::EID_PMU, pmu::NUM_COUNTERS, [0; 6]));
    // A firmware counter avoids stopping the cycle/instret counters used to
    // measure this kernel. Search one index at a time, including on RV32.
    let mut selected = None;
    for index in 0..count {
        let ret = call(pmu::EID_PMU, pmu::COUNTER_GET_INFO, [index, 0, 0, 0, 0, 0]);
        if ret.error == 0 && ret.value >> (usize::BITS - 1) != 0 {
            let ret = call(
                pmu::EID_PMU,
                pmu::COUNTER_CONFIG_MATCHING,
                [index, 1, 0, 0xf0000 | pmu::firmware_event::SET_TIMER, 0, 0],
            );
            if ret.error == SbiRet::not_supported().error {
                continue;
            }
            checked(ret);
            assert_eq!(ret.value, index);
            selected = Some(index);
            break;
        }
    }
    let Some(index) = selected else {
        println!("LINUX_SKIP pmu_fw_lifecycle reason=no-firmware-counter");
        return;
    };
    println!("LINUX_CONTEXT pmu counter={index} event=firmware-set-timer");
    let start = || call(pmu::EID_PMU, pmu::COUNTER_START, [index, 1, 1, 0, 0, 0]);
    let stop = || call(pmu::EID_PMU, pmu::COUNTER_STOP, [index, 1, 0, 0, 0, 0]);
    let reset = || call(pmu::EID_PMU, pmu::COUNTER_STOP, [index, 1, 1, 0, 0, 0]);
    // The counter starts stopped and remains allocated between these samples.
    measure(
        "pmu_start",
        "call",
        1,
        || {},
        start,
        |_| {
            checked(stop());
        },
    );
    measure(
        "pmu_stop",
        "call",
        1,
        || {
            checked(start());
        },
        stop,
        |_| {},
    );
    checked(start());
    scalar(
        "pmu_fw_read",
        pmu::EID_PMU,
        pmu::COUNTER_FW_READ,
        [index, 0, 0, 0, 0, 0],
    );
    #[cfg(target_pointer_width = "32")]
    scalar(
        "pmu_fw_read_hi",
        pmu::EID_PMU,
        pmu::COUNTER_FW_READ_HI,
        [index, 0, 0, 0, 0, 0],
    );
    checked(reset());
    measure(
        "pmu_config",
        "call",
        1,
        || {},
        || {
            call(
                pmu::EID_PMU,
                pmu::COUNTER_CONFIG_MATCHING,
                [index, 1, 0, 0xf0000 | pmu::firmware_event::SET_TIMER, 0, 0],
            )
        },
        |ret| {
            assert_eq!(checked(ret), index);
            checked(start());
            checked(reset());
        },
    );
}

#[repr(C, align(4096))]
struct SharedPage([u8; 4096]);
static mut SHARED: SharedPage = SharedPage([0; 4096]);

fn shared_registration(name: &str, eid: usize, fid: usize) {
    if !available(eid) {
        println!("LINUX_SKIP {name} reason=extension-unavailable");
        return;
    }
    let address = core::ptr::addr_of_mut!(SHARED) as usize;
    let ret = call(eid, fid, [address, 0, 0, 0, 0, 0]);
    if ret.error == SbiRet::not_supported().error {
        println!("LINUX_SKIP {name} reason=function-unavailable");
        return;
    }
    checked(ret);
    scalar(name, eid, fid, [address, 0, 0, 0, 0, 0]);
    // Disable the region before another extension uses it. Only raw pointers
    // refer to this page while firmware owns or writes the shared memory.
    checked(call(eid, fid, [usize::MAX, usize::MAX, 0, 0, 0, 0]));
}

fn optional_calls() {
    if available(fwft::EID_FWFT) {
        // Read the existing value first; SET never changes the boot policy or
        // locks the feature. Linux uses SET for misaligned trap delegation.
        let feature = fwft::feature_type::MISALIGNED_EXC_DELEG;
        let ret = call(fwft::EID_FWFT, fwft::GET, [feature, 0, 0, 0, 0, 0]);
        if ret.error == 0 {
            scalar(
                "fwft_set_current",
                fwft::EID_FWFT,
                fwft::SET,
                [feature, ret.value, 0, 0, 0, 0],
            );
        } else {
            println!(
                "LINUX_SKIP fwft_set_current reason=feature-unavailable error={}",
                ret.error as isize
            );
        }
    } else {
        println!("LINUX_SKIP fwft_set_current reason=extension-unavailable");
    }
    shared_registration("sta_set_shmem", sta::EID_STA, 0);
    shared_registration("pmu_snapshot_shmem", pmu::EID_PMU, pmu::SNAPSHOT_SET_SHMEM);
    // KVM and mailbox discovery calls are safe without a guest or channel.
    scalar("nacl_probe", 0x4e41434c, 0, [0; 6]);
    scalar("mpxy_shmem_size", 0x4d505859, 0, [0; 6]);
    println!("LINUX_SKIP srst_susp reason=nonreturning-system-lifecycle");
    println!("LINUX_SKIP nacl_sync_mpxy_messages reason=requires-guest-or-channel-fixture");
    println!("LINUX_SKIP pmu_event_info reason=requires-event-array-fixture");
}

fn console_calls() {
    if !available(dbcn::EID_DBCN) {
        println!("LINUX_SKIP dbcn_io reason=extension-unavailable");
        return;
    }
    // Short batches bound UART time and log volume. DBCN is nonblocking: sum
    // actual bytes accepted/read instead of assuming a full transfer per call.
    for (name, fid, address) in [
        ("dbcn_write_16", 0, b"linux-sbi-test!\r\n".as_ptr() as usize),
        ("dbcn_read_16", 1, core::ptr::addr_of_mut!(SHARED) as usize),
    ] {
        let mut bytes = 0usize;
        println!("LINUX_CONTEXT {name} requested_bytes=16");
        measure(
            name,
            "call",
            4,
            || {},
            || {
                let ret = call(dbcn::EID_DBCN, fid, [16, address, 0, 0, 0, 0]);
                bytes += ret.value;
                if ret.error == 0 && ret.value > 16 {
                    return SbiRet::failed();
                }
                ret
            },
            |_| {},
        );
        println!("\nLINUX_IO {name} accepted_bytes={bytes} includes_warmup=true");
    }
}

fn wait(mut done: impl FnMut() -> bool) {
    let start = riscv::register::time::read();
    while !done() {
        assert_eq!(
            ERROR.load(Ordering::Acquire),
            0,
            "secondary hart SBI failure"
        );
        assert!(
            riscv::register::time::read().wrapping_sub(start)
                < TIMEOUT_TICKS.load(Ordering::Relaxed),
            "Linux workload timeout"
        );
        core::hint::spin_loop();
    }
}

fn status(hart: usize, wanted: usize) -> bool {
    let ret = call(hsm::EID_HSM, hsm::HART_GET_STATUS, [hart, 0, 0, 0, 0, 0]);
    checked(ret) == wanted
}

fn stack(hart: usize) -> usize {
    assert!(hart < super::MAX_HART_NUM);
    // Each worker owns its indexed stack. None is reused until HSM STOPPED.
    unsafe { core::ptr::addr_of!(super::HART_STACK[hart]) as usize + super::STACK_SIZE }
}

#[unsafe(naked)]
unsafe extern "C" fn worker_entry(_hart: usize, _stack: usize) -> ! {
    naked_asm!("mv sp, a1", "mv tp, a0", "csrci sstatus, 2", "csrw sie, zero", "j {worker}", worker = sym worker);
}

#[unsafe(naked)]
unsafe extern "C" fn resume_entry(_hart: usize, _stack: usize) -> ! {
    naked_asm!("mv sp, a1", "mv tp, a0", "csrci sstatus, 2", "csrw sie, zero", "j {resume}", resume = sym resumed);
}

extern "C" fn resumed(hart: usize) -> ! {
    COMMAND.store(READY, Ordering::Release);
    worker(hart)
}

extern "C" fn worker(hart: usize) -> ! {
    ONLINE.store(hart + 1, Ordering::Release);
    loop {
        // Poll the S-mode pending bit with SIE disabled. M-mode still handles
        // IPIs/RFence. This measures delivery plus a polling acknowledgement,
        // not Linux's S-mode interrupt handler or scheduler.
        let pending: usize;
        unsafe {
            asm!("csrr {}, sip", out(reg) pending, options(nostack));
        }
        if pending & 2 != 0 {
            unsafe {
                asm!("csrci sip, 2", options(nostack));
            }
            ACK.fetch_add(1, Ordering::Release);
        }
        match COMMAND.load(Ordering::Acquire) {
            STOP => {
                let ret = call(hsm::EID_HSM, hsm::HART_STOP, [0; 6]);
                ERROR.store(ret.error | 1, Ordering::Release);
            }
            RETENTIVE | NON_RETENTIVE => {
                let kind = COMMAND.load(Ordering::Acquire);
                let suspend_type = if kind == RETENTIVE { 0 } else { 0x8000_0000 };
                let ret = call(
                    hsm::EID_HSM,
                    hsm::HART_SUSPEND,
                    [
                        suspend_type,
                        resume_entry as *const () as usize,
                        stack(hart),
                        0,
                        0,
                        0,
                    ],
                );
                if ret.error != 0 {
                    ERROR.store(ret.error, Ordering::Release);
                }
                COMMAND.store(READY, Ordering::Release);
            }
            _ => core::hint::spin_loop(),
        }
    }
}

fn start_worker(hart: usize) {
    ONLINE.store(0, Ordering::Relaxed);
    COMMAND.store(READY, Ordering::Release);
    checked(call(
        hsm::EID_HSM,
        hsm::HART_START,
        [
            hart,
            worker_entry as *const () as usize,
            stack(hart),
            0,
            0,
            0,
        ],
    ));
    wait(|| ONLINE.load(Ordering::Acquire) == hart + 1);
}

fn stop_worker(hart: usize) {
    COMMAND.store(STOP, Ordering::Release);
    wait(|| status(hart, hsm::hart_state::STOPPED));
}

fn send_and_ack(hart: usize) -> SbiRet {
    let expected = ACK.load(Ordering::Acquire).wrapping_add(1);
    let ret = call(ipi::EID_SPI, ipi::SEND_IPI, [1, hart, 0, 0, 0, 0]);
    if ret.error == 0 {
        wait(|| ACK.load(Ordering::Acquire) == expected);
    }
    ret
}

/// Check suspend outside measurement. An unobservable suspended state is not
/// a wake-latency sample: recover the worker with an IPI and report the gap.
fn suspend_supported(hart: usize, kind: usize, name: &str) -> bool {
    COMMAND.store(kind, Ordering::Release);
    let start = riscv::register::time::read();
    let last_status = loop {
        let state = checked(call(
            hsm::EID_HSM,
            hsm::HART_GET_STATUS,
            [hart, 0, 0, 0, 0, 0],
        ));
        if state == hsm::hart_state::SUSPENDED {
            checked(send_and_ack(hart));
            wait(|| COMMAND.load(Ordering::Acquire) == READY);
            return true;
        }
        if ERROR.load(Ordering::Acquire) != 0
            || riscv::register::time::read().wrapping_sub(start)
                >= TIMEOUT_TICKS.load(Ordering::Relaxed)
        {
            break state;
        }
        core::hint::spin_loop();
    };
    // Do not reuse a worker/stack until it has returned from suspend. A
    // recovery failure still aborts the run instead of hiding a stuck hart.
    let recovery_start = riscv::register::time::read();
    let expected_ack = ACK.load(Ordering::Acquire).wrapping_add(1);
    checked(call(ipi::EID_SPI, ipi::SEND_IPI, [1, hart, 0, 0, 0, 0]));
    while COMMAND.load(Ordering::Acquire) != READY || ACK.load(Ordering::Acquire) != expected_ack {
        assert!(
            riscv::register::time::read().wrapping_sub(recovery_start)
                < TIMEOUT_TICKS.load(Ordering::Relaxed),
            "suspend recovery timeout on hart {hart}"
        );
        core::hint::spin_loop();
    }
    let error = ERROR.swap(0, Ordering::AcqRel);
    println!(
        "LINUX_SKIP {name} reason=suspend-state-unavailable hart={hart} status={last_status} error={} recovered=true",
        error as isize
    );
    false
}

fn fences(target: &str, mask: usize, hart: usize) {
    if !available(rfnc::EID_RFNC) {
        return;
    }
    for (operation, fid, start, size, asid) in [
        ("fence_i", 0, 0, 0, 0),
        ("sfence_page", 1, 0x4000, 4096, 0),
        ("sfence_range", 1, 0x4000, 16 * 4096, 0),
        ("sfence_all", 1, 0, usize::MAX, 0),
        ("sfence_asid_page", 2, 0x4000, 4096, 1),
        ("sfence_asid_all", 2, 0, usize::MAX, 1),
        ("hfence_gvma_vmid", 3, 0, usize::MAX, 1),
        ("hfence_gvma", 4, 0, usize::MAX, 0),
        ("hfence_vvma_asid", 5, 0, usize::MAX, 1),
        ("hfence_vvma", 6, 0, usize::MAX, 0),
    ] {
        println!("LINUX_CONTEXT {operation} target={target} mask={mask:#x} base={hart}");
        scalar(
            operation,
            rfnc::EID_RFNC,
            fid,
            [mask, hart, start, size, asid, 0],
        );
    }
}

pub(super) fn run(hart: usize, smp: usize, frequency: u32) -> bool {
    println!(
        "Linux SBI workloads: timebase={frequency} Hz; units distinguish calls from complete operations"
    );
    assert!(frequency != 0 && smp <= super::MAX_HART_NUM);
    TIMEOUT_TICKS.store((frequency as usize).saturating_mul(5), Ordering::Relaxed);
    measure(
        "counter_overhead",
        "sample",
        1,
        || {},
        || SbiRet::success(0),
        |_| {},
    );
    for (name, fid) in [
        ("base_impl_id", 1),
        ("base_impl_version", 2),
        ("base_marchid", 5),
        ("base_mimpid", 6),
    ] {
        scalar(name, base::EID_BASE, fid, [0; 6]);
    }
    // Unlike time_set in the compatibility suite, use a finite deadline.
    let deadline = riscv::register::time::read64() + u64::from(frequency) * 60;
    let high = if usize::BITS == 32 {
        (deadline >> 32) as usize
    } else {
        0
    };
    scalar(
        "time_deadline",
        0x54494d45,
        0,
        [deadline as usize, high, 0, 0, 0, 0],
    );
    if available(0x54494d45) {
        checked(call(0x54494d45, 0, [usize::MAX, usize::MAX, 0, 0, 0, 0]));
    }
    fences("self", 1, hart);
    pmu_calls();
    optional_calls();
    console_calls();
    let mut suspend_complete = true;
    if available(hsm::EID_HSM) && available(ipi::EID_SPI) {
        for remote in 0..smp {
            if remote == hart {
                continue;
            }
            println!("LINUX_CONTEXT remote hart={remote} boot_hart={hart}");
            assert!(status(remote, hsm::hart_state::STOPPED));
            measure(
                "hsm_start_ready",
                "start_to_ready",
                1,
                || {},
                || {
                    start_worker(remote);
                    SbiRet::success(0)
                },
                |_| stop_worker(remote),
            );
            measure(
                "hsm_stop_observed",
                "stop_to_observed",
                1,
                || start_worker(remote),
                || {
                    stop_worker(remote);
                    SbiRet::success(0)
                },
                |_| {},
            );
            start_worker(remote);
            scalar(
                "hsm_status_remote",
                hsm::EID_HSM,
                hsm::HART_GET_STATUS,
                [remote, 0, 0, 0, 0, 0],
            );
            measure(
                "ipi_remote_ack",
                "send_to_ack",
                BATCH,
                || {},
                || send_and_ack(remote),
                |_| {},
            );
            fences("remote", 1, remote);
            stop_worker(remote);
        }
        if smp > 1 && smp <= usize::BITS as usize {
            for remote in 0..smp {
                if remote != hart {
                    start_worker(remote);
                }
            }
            let all = usize::MAX >> (usize::BITS as usize - smp);
            fences("all_remote", all & !(1 << hart), 0);
            COMMAND.store(STOP, Ordering::Release);
            for remote in 0..smp {
                if remote != hart {
                    wait(|| status(remote, hsm::hart_state::STOPPED));
                }
            }
        }
        // Suspend runs last so a firmware that cannot recover a suspended
        // worker cannot prevent ordinary remote-call measurements.
        for remote in 0..smp {
            if remote == hart {
                continue;
            }
            println!("LINUX_CONTEXT remote hart={remote} boot_hart={hart}");
            start_worker(remote);
            // Waiting for SUSPENDED happens outside timing; report wake-to-ack
            // rather than hart_suspend cost.
            for (name, kind) in [
                ("hsm_retentive_wake", RETENTIVE),
                ("hsm_nonretentive_wake", NON_RETENTIVE),
            ] {
                if !suspend_supported(remote, kind, name) {
                    suspend_complete = false;
                    continue;
                }
                measure(
                    name,
                    "wake_to_ack",
                    1,
                    || {
                        COMMAND.store(kind, Ordering::Release);
                        wait(|| status(remote, hsm::hart_state::SUSPENDED));
                    },
                    || send_and_ack(remote),
                    |_| wait(|| COMMAND.load(Ordering::Acquire) == READY),
                );
            }
            stop_worker(remote);
        }
    } else {
        println!("LINUX_SKIP remote_harts reason=hsm-or-ipi-unavailable");
    }
    let result = if suspend_complete { "PASS" } else { "PARTIAL" };
    println!("Linux SBI workloads completed: {result}");
    suspend_complete
}
