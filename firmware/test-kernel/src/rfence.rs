//! SBI remote-fence and hypervisor-fence smoke tests.

use crate::pmu;
use core::{
    arch::naked_asm,
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
};
use sbi_spec::binary::{HartMask, SbiRet};
use sbi_spec::pmu::firmware_event;
use sbi_testing::sbi;

#[cfg(feature = "hypervisor")]
mod hypervisor;

static READY: AtomicBool = AtomicBool::new(false);
static STOP: AtomicBool = AtomicBool::new(false);
static CONCURRENT: AtomicBool = AtomicBool::new(false);
static CONCURRENT_MASK: AtomicUsize = AtomicUsize::new(0);
static FENCE_ROUND: [AtomicUsize; 2] = [const { AtomicUsize::new(0) }; 2];
static FENCE_PMU_COMMAND: AtomicUsize = AtomicUsize::new(0);
static FENCE_PMU_ARGUMENT: AtomicUsize = AtomicUsize::new(0);
static FENCE_PMU_RESULT: AtomicUsize = AtomicUsize::new(0);
static FENCE_PMU_DONE: AtomicBool = AtomicBool::new(false);

const FENCE_PMU_START: usize = 1;
const FENCE_PMU_STOP: usize = 2;

#[repr(align(16))]
struct SecondaryStack([u8; 16384]);

static mut SECONDARY_STACK: SecondaryStack = SecondaryStack([0; 16384]);

/// # Safety
/// Start only one secondary at a time, with a1 pointing past SECONDARY_STACK.
/// The boot hart must wait for STOPPED before reusing the stack.
#[unsafe(naked)]
unsafe extern "C" fn secondary_entry(_: usize, _: usize) -> ! {
    naked_asm!(
        "mv sp, a1",
        "csrw sie, zero",
        "j {run}",
        run = sym secondary_run,
    );
}

extern "C" fn secondary_run() -> ! {
    #[cfg(feature = "hypervisor")]
    let mut guest = hypervisor::install_secondary();
    READY.store(true, Ordering::Release);
    while !STOP.load(Ordering::Acquire) {
        if CONCURRENT.load(Ordering::Acquire) {
            let mask = HartMask::from_mask_base(CONCURRENT_MASK.load(Ordering::Relaxed), 0);
            concurrent_fences(mask, 1);
            CONCURRENT.store(false, Ordering::Release);
        }
        let command = FENCE_PMU_COMMAND.swap(0, Ordering::Acquire);
        if command != 0 {
            let argument = FENCE_PMU_ARGUMENT.load(Ordering::Relaxed);
            let result = match command {
                FENCE_PMU_START => pmu::start_firmware_event(argument).map_or(0, |idx| idx + 1),
                FENCE_PMU_STOP => pmu::stop_firmware_event(argument),
                _ => panic!("unknown RFENCE PMU command"),
            };
            FENCE_PMU_RESULT.store(result, Ordering::Relaxed);
            FENCE_PMU_DONE.store(true, Ordering::Release);
        }
        #[cfg(feature = "hypervisor")]
        hypervisor::secondary_poll(&mut guest);
        core::hint::spin_loop();
    }
    #[cfg(feature = "hypervisor")]
    guest.restore();
    let ret = sbi::hart_stop();
    panic!("RFENCE secondary hart_stop returned: {ret:?}");
}

fn ordinary_fences(mask: HartMask) {
    assert_eq!(sbi::remote_fence_i(mask), SbiRet::success(0));
    for (start, size) in [(0, 0), (0x1000, 1), (0x1000, 0x1001), (1, usize::MAX)] {
        assert_eq!(
            sbi::remote_sfence_vma(mask, start, size),
            SbiRet::success(0)
        );
        for asid in [0, 1, usize::MAX] {
            assert_eq!(
                sbi::remote_sfence_vma_asid(mask, start, size, asid),
                SbiRet::success(0)
            );
        }
    }
}

fn secondary_pmu_command(command: usize, argument: usize) -> usize {
    FENCE_PMU_DONE.store(false, Ordering::Relaxed);
    FENCE_PMU_ARGUMENT.store(argument, Ordering::Relaxed);
    FENCE_PMU_COMMAND.store(command, Ordering::Release);
    while !FENCE_PMU_DONE.load(Ordering::Acquire) {
        core::hint::spin_loop();
    }
    FENCE_PMU_RESULT.load(Ordering::Relaxed)
}

fn assert_remote_fence_received(
    event: usize,
    operation: &str,
    invoke: impl FnOnce() -> SbiRet,
) -> bool {
    let Some(counter) = secondary_pmu_command(FENCE_PMU_START, event).checked_sub(1) else {
        return false;
    };
    assert_eq!(invoke(), SbiRet::success(0), "remote {operation} failed");
    let received = secondary_pmu_command(FENCE_PMU_STOP, counter);
    assert_eq!(
        received, 1,
        "target must receive exactly one remote {operation} request"
    );
    true
}

fn test_remote_fence_receipts(mask: HartMask) {
    let observed = [
        assert_remote_fence_received(firmware_event::FENCE_I_RECEIVED, "FENCE.I", || {
            sbi::remote_fence_i(mask)
        }),
        assert_remote_fence_received(firmware_event::SFENCE_VMA_RECEIVED, "SFENCE.VMA", || {
            sbi::remote_sfence_vma(mask, 0x1000, 0x1000)
        }),
        assert_remote_fence_received(
            firmware_event::SFENCE_VMA_ASID_RECEIVED,
            "SFENCE.VMA with ASID",
            || sbi::remote_sfence_vma_asid(mask, 0x1000, 0x1000, 1),
        ),
    ];
    if observed.into_iter().all(|received| received) {
        println!("[rfence] remote ordinary fence receipt counters pass");
    } else {
        println!("[rfence] remote ordinary fence receipt counters unavailable");
    }
}

/// Both harts enter each round before making a synchronous, bidirectional call.
/// Waiting firmware must service inbound work to avoid mutual starvation.
fn concurrent_fences(mask: HartMask, participant: usize) {
    for round in 1..=48 {
        FENCE_ROUND[participant].store(round, Ordering::Release);
        while FENCE_ROUND[1 - participant].load(Ordering::Acquire) < round {
            core::hint::spin_loop();
        }
        let result = match round % 3 {
            0 => sbi::remote_fence_i(mask),
            1 => sbi::remote_sfence_vma(mask, 0, 4096),
            _ => sbi::remote_sfence_vma_asid(mask, 0, 4096, round),
        };
        assert_eq!(result, SbiRet::success(0));
    }
}

fn active_secondary(hartid: usize, smp: usize) {
    let Some(target) = (0..smp).find(|&target| {
        target != hartid
            && sbi::hart_get_status(target) == SbiRet::success(sbi_spec::hsm::hart_state::STOPPED)
    }) else {
        assert_eq!(smp, 1, "RFENCE test requires a stopped secondary on SMP");
        println!("[rfence] active-hart test skipped: no stopped secondary");
        return;
    };
    READY.store(false, Ordering::Relaxed);
    STOP.store(false, Ordering::Relaxed);
    // SAFETY: No secondary is using this permanent, ABI-aligned stack; this
    // test waits for STOPPED before allowing another start or test to use it.
    let top = unsafe { (&raw mut SECONDARY_STACK.0).cast::<u8>().add(16384) as usize };
    assert_eq!(
        sbi::hart_start(target, secondary_entry as *const () as usize, top),
        SbiRet::success(0)
    );
    while !READY.load(Ordering::Acquire) {
        core::hint::spin_loop();
    }
    assert_eq!(
        sbi::hart_get_status(target),
        SbiRet::success(sbi_spec::hsm::hart_state::STARTED)
    );
    let remote = HartMask::from_mask_base(1, target);
    let mixed = HartMask::from_mask_base((1 << hartid) | (1 << target), 0);
    for _ in 0..4 {
        ordinary_fences(remote);
        ordinary_fences(mixed);
    }
    test_remote_fence_receipts(remote);
    for round in &FENCE_ROUND {
        round.store(0, Ordering::Relaxed);
    }
    CONCURRENT_MASK.store((1 << hartid) | (1 << target), Ordering::Relaxed);
    CONCURRENT.store(true, Ordering::Release);
    concurrent_fences(mixed, 0);
    while CONCURRENT.load(Ordering::Acquire) {
        core::hint::spin_loop();
    }
    println!("[rfence] concurrent callers complete: 48 rounds per hart");
    #[cfg(feature = "hypervisor")]
    hypervisor::test_remote(remote, mixed);
    STOP.store(true, Ordering::Release);
    while sbi::hart_get_status(target) != SbiRet::success(sbi_spec::hsm::hart_state::STOPPED) {
        core::hint::spin_loop();
    }
    println!("[rfence] active-hart delivery and completion pass: hart {target}");
}

/// Exercises the RFENCE calls for the current hart and checks an invalid
/// hart ID for the calls whose target validation is observable.
///
/// Uses the test kernel's `0..smp` hart IDs when building SBI masks.
pub(crate) fn test(hartid: usize, smp: usize) {
    let self_mask = HartMask::from_mask_base(0x1, hartid);
    let invalid_hart = smp + 1;

    // A stopped hart remains assigned to the supervisor. It can be skipped
    // without rejecting an otherwise valid IPI or remote-fence mask.
    for target in 0..smp {
        if target == hartid
            || sbi::hart_get_status(target) != SbiRet::success(sbi_spec::hsm::hart_state::STOPPED)
        {
            continue;
        }
        let stopped = HartMask::from_mask_base(1, target);
        assert_eq!(sbi::send_ipi(stopped), SbiRet::success(0));
        let mixed = HartMask::from_mask_base((1 << hartid) | (1 << target), 0);
        for mask in [stopped, mixed] {
            assert_eq!(sbi::remote_fence_i(mask), SbiRet::success(0));
            assert_eq!(sbi::remote_sfence_vma(mask, 0, 0), SbiRet::success(0));
            assert_eq!(
                sbi::remote_sfence_vma_asid(mask, 0, 0, 0),
                SbiRet::success(0)
            );
        }
        println!("Sbi stopped-hart IPI/RFENCE test pass: hart {target}");
    }

    // Fence.i is allowed to be unsupported by a platform.
    let ret = sbi::remote_fence_i(self_mask);
    assert!(ret.is_ok() || ret == SbiRet::not_supported());
    let ret = sbi::remote_fence_i(HartMask::from_mask_base(0x1, invalid_hart));
    assert_eq!(ret, SbiRet::invalid_param());

    // SFence.vma is allowed to be unsupported by a platform.
    let ret = sbi::remote_sfence_vma(self_mask, 0, 0);
    assert!(ret.is_ok() || ret == SbiRet::not_supported());
    let ret = sbi::remote_sfence_vma(HartMask::from_mask_base(0x1, invalid_hart), 0, 0);
    assert_eq!(ret, SbiRet::invalid_param());

    // size == usize::MAX is a full-address-space encoding: start_addr is
    // ignored, including nonzero and unaligned values (#484).
    let ret = sbi::remote_sfence_vma(self_mask, 8, usize::MAX);
    assert!(ret.is_ok() || ret == SbiRet::not_supported());
    let ret = sbi::remote_sfence_vma(self_mask, 1, usize::MAX);
    assert!(ret.is_ok() || ret == SbiRet::not_supported());
    let ret = sbi::remote_sfence_vma_asid(self_mask, 8, usize::MAX, 0);
    assert!(ret.is_ok() || ret == SbiRet::not_supported());

    let ret = sbi::remote_sfence_vma_asid(self_mask, 0, 0, 0);
    assert!(ret.is_ok() || ret == SbiRet::not_supported());

    for (start, size) in [(1, 0x1000), (usize::MAX & !0xfff, 0x1000)] {
        assert_eq!(
            sbi::remote_sfence_vma(self_mask, start, size),
            SbiRet::invalid_address()
        );
        assert_eq!(
            sbi::remote_sfence_vma_asid(self_mask, start, size, 0),
            SbiRet::invalid_address()
        );
    }
    ordinary_fences(self_mask);
    println!("[rfence] local range validation pass");
    #[cfg(feature = "hypervisor")]
    hypervisor::initialize();
    active_secondary(hartid, smp);

    #[cfg(feature = "hypervisor")]
    hypervisor::test_local(self_mask);

    #[cfg(not(feature = "hypervisor"))]
    hypervisor_smoke(self_mask);

    println!("[34m[ INFO] Sbi `RFNC` test pass[0m");
}

#[cfg(not(feature = "hypervisor"))]
fn hypervisor_smoke(self_mask: HartMask) {
    let empty_mask = HartMask::from_mask_base(0, 0);
    let results = [
        sbi::remote_hfence_gvma(self_mask, 0, 0),
        sbi::remote_hfence_gvma_vmid(self_mask, 0, 0, 0),
        sbi::remote_hfence_vvma(self_mask, 0, 0),
        sbi::remote_hfence_vvma_asid(self_mask, 0, 0, 0),
        // Empty requests still require H and a published SBI service.
        sbi::remote_hfence_gvma(self_mask, 0x1000, 0),
        sbi::remote_hfence_gvma_vmid(self_mask, 0x1000, 0, 0),
        sbi::remote_hfence_vvma(self_mask, 0x1000, 0),
        sbi::remote_hfence_vvma_asid(self_mask, 0x1000, 0, 0),
        // An empty target mask does not bypass the calling hart's H check.
        sbi::remote_hfence_gvma(empty_mask, 0, 0),
        sbi::remote_hfence_gvma_vmid(empty_mask, 0, 0, 0),
        sbi::remote_hfence_vvma(empty_mask, 0, 0),
        sbi::remote_hfence_vvma_asid(empty_mask, 0, 0, 0),
    ];
    // The generic kernel accepts optional H, but prints a distinct marker so
    // the dedicated runner can require NOT_SUPPORTED in its negative cases.
    if results.iter().all(|ret| *ret == SbiRet::not_supported()) {
        println!("[rfence] H unavailable returns NOT_SUPPORTED");
    } else {
        assert!(results.iter().all(|ret| *ret == SbiRet::success(0)));
        println!("[rfence] optional H calls return SUCCESS");
    }
}
