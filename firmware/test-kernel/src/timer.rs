//! Supervisor deadline reprogramming with timer interrupts masked.

use csr::{
    machine::{Interrupt, PendingState},
    supervisor,
    user::Time,
};
use sbi_testing::sbi;

pub(crate) fn test(frequency: u64) {
    if sbi::probe_extension(sbi::Timer).is_unavailable() {
        println!("[timer] TIME unavailable");
        return;
    }
    let interval = (frequency / 100).max(1);
    // SAFETY: The test excludes supervisor interrupt reentry during polling.
    let status = unsafe { supervisor::Status::clear_bit::<supervisor::SIE>() };
    {
        let was_enabled =
            supervisor::InterruptEnable::read().get::<supervisor::STIE>() == Interrupt::Enabled;
        // SAFETY: This suite masks STIE while polling pending state. It restores
        // the original enable only after cancelling and observing no pending timer.
        unsafe { supervisor::InterruptEnable::clear_bit::<supervisor::STIE>() };
        for _ in 0..3 {
            set_timer(0);
            wait_pending(true, frequency);
            set_timer(Time::read64().saturating_add(frequency));
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
            unsafe { supervisor::InterruptEnable::set_bit::<supervisor::STIE>() };
        }
    }
    if status.get::<supervisor::SIE>() == Interrupt::Enabled {
        // SAFETY: The original interrupt state is restored after cancellation.
        unsafe { supervisor::Status::set_bit::<supervisor::SIE>() };
    }
    println!("[timer] expired, rearmed, and cancelled deadlines pass");
}

fn set_timer(deadline: u64) {
    assert!(
        sbi::set_timer(deadline).is_ok(),
        "SBI TIME rejected a deadline"
    );
}

fn wait_pending(pending: bool, timeout: u64) {
    let start = Time::read64();
    while (supervisor::InterruptPending::read().get::<supervisor::STIP>() == PendingState::Pending)
        != pending
    {
        assert!(
            Time::read64().wrapping_sub(start) < timeout,
            "timer pending state did not become {pending}"
        );
        core::hint::spin_loop();
    }
}

fn expect_clear(interval: u64) {
    let start = Time::read64();
    while Time::read64().wrapping_sub(start) < interval {
        assert!(
            supervisor::InterruptPending::read().get::<supervisor::STIP>() != PendingState::Pending,
            "cancelled timer became pending again"
        );
        core::hint::spin_loop();
    }
}
