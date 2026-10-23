//! FWFT ABI and hart-local lock checks through real SBI calls.

use core::{
    arch::naked_asm,
    sync::atomic::{AtomicUsize, Ordering},
};
use sbi_spec::{
    binary::{HartMask, SbiRet},
    fwft::feature_type,
    hsm::hart_state,
};
use sbi_testing::sbi;

const MISALIGNED: u32 = feature_type::MISALIGNED_EXC_DELEG as u32;
static PHASE: AtomicUsize = AtomicUsize::new(0);
static RESUME_COUNTER: AtomicUsize = AtomicUsize::new(usize::MAX);

#[repr(align(16))]
struct Stack {
    _bytes: [u8; 16384],
}
static mut STACK: Stack = Stack { _bytes: [0; 16384] };

pub(crate) fn test(hart: usize, smp: usize, frequency: u64) {
    if sbi::probe_extension(sbi::Fwft).is_unavailable() {
        println!("[fwft] unavailable");
        return;
    }
    assert_eq!(sbi::fwft_get(6), SbiRet::denied());
    assert_eq!(sbi::fwft_set(6, 0, 0), SbiRet::denied());
    for feature in 0..=5 {
        let previous = sbi::fwft_get(feature);
        if previous == SbiRet::not_supported() {
            // A rejected hardware update must not commit the requested lock.
            assert_eq!(sbi::fwft_set(feature, 0, 1), SbiRet::not_supported());
            assert_eq!(sbi::fwft_set(feature, 0, 0), SbiRet::not_supported());
            continue;
        }
        assert!(previous.is_ok(), "FWFT get {feature}: {previous:?}");
        assert_eq!(
            sbi::fwft_set(feature, previous.value, 0),
            SbiRet::success(0)
        );
        assert_eq!(
            sbi::fwft_set(feature, usize::MAX, 1),
            SbiRet::invalid_param()
        );
        assert_eq!(
            sbi::fwft_set(feature, previous.value, 2),
            SbiRet::invalid_param()
        );
        assert_eq!(sbi::fwft_get(feature), previous);
        if feature == feature_type::POINTER_MASKING_PMLEN as u32 {
            for length in [0, 7, 16] {
                let result = sbi::fwft_set(feature, length, 0);
                if result.is_ok() {
                    assert_eq!(sbi::fwft_get(feature), SbiRet::success(length));
                    assert_eq!(
                        sbi::fwft_set(feature, previous.value, 0),
                        SbiRet::success(0)
                    );
                } else {
                    assert_eq!(result, SbiRet::invalid_param());
                    assert_eq!(sbi::fwft_get(feature), previous);
                }
            }
            println!("[fwft] pointer masking lengths pass");
        }
        // These transitions do not require a CFI-enabled test kernel.
        if feature == MISALIGNED || feature == feature_type::PTE_AD_HW_UPDATING as u32 {
            assert_eq!(
                sbi::fwft_set(feature, previous.value ^ 1, 0),
                SbiRet::success(0)
            );
            assert_eq!(sbi::fwft_get(feature), SbiRet::success(previous.value ^ 1));
            assert_eq!(
                sbi::fwft_set(feature, previous.value, 0),
                SbiRet::success(0)
            );
            if feature == feature_type::PTE_AD_HW_UPDATING as u32 {
                println!("[fwft] PTE A/D switching pass");
            }
        }
    }
    if let Some(remote) = (0..smp).find(|&other| other != hart) {
        assert_eq!(
            sbi::hart_start(remote, entry as *const () as usize, 0),
            SbiRet::success(0)
        );
        wait(frequency, || PHASE.load(Ordering::Acquire) == 1);
        let own = sbi::fwft_get(MISALIGNED);
        assert!(own.is_ok());
        // The remote hart's lock must not prevent this hart's updates.
        assert_eq!(
            sbi::fwft_set(MISALIGNED, own.value ^ 1, 0),
            SbiRet::success(0)
        );
        assert_eq!(sbi::fwft_set(MISALIGNED, own.value, 0), SbiRet::success(0));
        wake_suspended(remote, frequency);
        wait(frequency, || PHASE.load(Ordering::Acquire) == 2);
        wake_suspended(remote, frequency);
        wait(frequency, || {
            sbi::hart_get_status(remote) == SbiRet::success(hart_state::STOPPED)
        });
        assert_eq!(
            sbi::hart_start(remote, entry as *const () as usize, 2),
            SbiRet::success(0)
        );
        wait(frequency, || PHASE.load(Ordering::Acquire) == 3);
        wait(frequency, || {
            sbi::hart_get_status(remote) == SbiRet::success(hart_state::STOPPED)
        });
        println!("[fwft] hart isolation, suspend, and restart locks pass");
    }
    // Permanent locks are checked last, after the trap/misaligned suites.
    let previous = sbi::fwft_get(MISALIGNED);
    assert!(previous.is_ok());
    assert_eq!(
        sbi::fwft_set(MISALIGNED, previous.value, 1),
        SbiRet::success(0)
    );
    assert_eq!(sbi::fwft_get(MISALIGNED), previous);
    assert_eq!(
        sbi::fwft_set(MISALIGNED, previous.value ^ 1, 0),
        SbiRet::denied_locked()
    );
    assert_eq!(sbi::fwft_get(MISALIGNED), previous);
    println!("[fwft] feature values and locks pass");
}

fn wait(frequency: u64, mut condition: impl FnMut() -> bool) {
    let start = riscv::register::time::read64();
    while !condition() {
        assert!(
            riscv::register::time::read64().wrapping_sub(start) < frequency * 5,
            "FWFT lifecycle test timed out"
        );
        core::hint::spin_loop();
    }
}

fn wake_suspended(hart: usize, frequency: u64) {
    wait(frequency, || {
        sbi::hart_get_status(hart) == SbiRet::success(hart_state::SUSPENDED)
    });
    assert_eq!(
        sbi::send_ipi(HartMask::from_mask_base(1, hart)),
        SbiRet::success(0)
    );
}

/// Enters the FWFT worker in S-mode with hart ID and phase in a0/a1.
///
/// # Safety
/// The caller must exclusively own STACK. The previous worker must have
/// stopped using it before another hart or phase enters this function.
#[unsafe(naked)]
unsafe extern "C" fn entry(_hart: usize, _phase: usize) -> ! {
    naked_asm!(
        "csrw sie, zero",
        "mv tp, a0",
        "lla sp, {stack} + {size}",
        "j {worker}",
        stack = sym STACK,
        size = const core::mem::size_of::<Stack>(),
        worker = sym worker,
    );
}

extern "C" fn worker(_hart: usize, phase: usize) -> ! {
    if phase == 0 {
        RESUME_COUNTER.store(
            crate::pmu::prepare_resume_counter().unwrap_or(usize::MAX),
            Ordering::Relaxed,
        );
        assert_eq!(sbi::fwft_set(MISALIGNED, 1, 1), SbiRet::success(0));
        PHASE.store(1, Ordering::Release);
        let ret = sbi::hart_suspend(sbi::NonRetentive, entry as *const () as usize, 1);
        panic!("FWFT non-retentive suspend returned: {ret:?}");
    }
    check_retained_lock();
    if phase == 1 {
        let counter = RESUME_COUNTER.load(Ordering::Relaxed);
        if counter != usize::MAX {
            crate::pmu::check_resume_counter(counter);
        }
        PHASE.store(2, Ordering::Release);
        assert_eq!(sbi::hart_suspend(sbi::Retentive, 0, 0), SbiRet::success(0));
        check_retained_lock();
    } else {
        PHASE.store(3, Ordering::Release);
    }
    let ret = sbi::hart_stop();
    panic!("FWFT hart stop returned: {ret:?}");
}

fn check_retained_lock() {
    assert_eq!(sbi::fwft_get(MISALIGNED), SbiRet::success(1));
    assert_eq!(sbi::fwft_set(MISALIGNED, 0, 0), SbiRet::denied_locked());
}
