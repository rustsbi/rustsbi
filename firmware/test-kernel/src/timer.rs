//! Supervisor deadline reprogramming with timer interrupts masked.

use riscv::register::{sie, sip, time};
use sbi_testing::sbi;

pub(crate) fn test(frequency: u64) {
    if sbi::probe_extension(sbi::Timer).is_unavailable() {
        println!("[timer] TIME unavailable");
        return;
    }
    let interval = (frequency / 100).max(1);
    riscv::interrupt::supervisor::free(|| {
        let was_enabled = sie::read().stimer();
        // SAFETY: This suite masks STIE while polling pending state. It restores
        // the original enable only after cancelling and observing no pending timer.
        unsafe { sie::clear_stimer() };
        for _ in 0..3 {
            set_timer(0);
            wait_pending(true, frequency);
            set_timer(time::read64().saturating_add(frequency));
            wait_pending(false, frequency);
            expect_clear(interval);

            set_timer(0);
            wait_pending(true, frequency);
            set_timer(u64::MAX);
            wait_pending(false, frequency);
            expect_clear(interval);
        }
        if was_enabled {
            // SAFETY: The original timer enable is restored with no pending deadline.
            unsafe { sie::set_stimer() };
        }
    });
    println!("[timer] expired, rearmed, and cancelled deadlines pass");
}

fn set_timer(deadline: u64) {
    assert!(
        sbi::set_timer(deadline).is_ok(),
        "SBI TIME rejected a deadline"
    );
}

fn wait_pending(pending: bool, timeout: u64) {
    let start = time::read64();
    while sip::read().stimer() != pending {
        assert!(
            time::read64().wrapping_sub(start) < timeout,
            "timer pending state did not become {pending}"
        );
        core::hint::spin_loop();
    }
}

fn expect_clear(interval: u64) {
    let start = time::read64();
    while time::read64().wrapping_sub(start) < interval {
        assert!(
            !sip::read().stimer(),
            "cancelled timer became pending again"
        );
        core::hint::spin_loop();
    }
}
