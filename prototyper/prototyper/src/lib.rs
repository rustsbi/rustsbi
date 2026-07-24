#![doc = include_str!("../../README.md")]
#![no_std]
#![forbid(unsafe_code)]

extern crate alloc;

use alloc::boxed::Box;

mod logger;
#[cfg(feature = "mtest")]
mod mtest;
mod platform;
mod sbi;

/// Initializes firmware policy and enters the configured next stage.
pub fn run(mut boot: machine::BootInfo) -> ! {
    let mut tree = platform::parse(&*boot.dtb_mut()).unwrap_or_else(|_| fail());
    let supervisor_ram = platform::memory(&tree).unwrap_or_else(|_| fail());
    let discovered_harts = platform::discover_harts(&tree).unwrap_or_else(|_| fail());
    let hart_count = discovered_harts.len();

    let (timer, ipi, fence, harts) =
        platform::install_timer_and_ipi(&mut boot, &mut tree, &discovered_harts)
            .unwrap_or_else(|_| fail());
    let console = platform::install_console(&mut tree).unwrap_or_else(|_| fail());
    if let Some(console) = console.as_ref() {
        logger::install(console.clone(), hart_count).unwrap_or_else(|_| fail());
    }
    let power = platform::install_power(&mut tree).unwrap_or_else(|_| fail());

    let protection = machine::pmp::config! {
        supervisor_ram => [read, write, execute];
    }
    .unwrap_or_else(|_| fail());
    boot.set_memory_protection(protection)
        .unwrap_or_else(|_| fail());
    let memory = boot.supervisor_memory().unwrap_or_else(|_| fail());
    let counters = boot
        .performance_counters()
        .map(Some)
        .unwrap_or_else(|_| fail());
    let monitor = sbi::prepare_performance_monitor(counters, hart_count).unwrap_or_else(|_| fail());

    platform::finish_device_tree(tree, boot.dtb_mut()).unwrap_or_else(|_| fail());

    let dispatcher = sbi::Dispatcher::new()
        .timer(timer)
        .ipi(ipi)
        .hart_control(harts)
        .remote_fence(fence)
        .system_reset(power)
        .debug_console(console, memory)
        .performance_monitor(monitor);
    boot.enter_next_stage(Box::new(dispatcher))
}

/// Runs the selected M-mode test in the dedicated test firmware.
#[cfg(feature = "mtest")]
pub fn run_mtest(boot: machine::BootInfo) -> ! {
    mtest::run(boot)
}

fn fail<T>() -> T {
    machine::abort(|| {})
}

/// Emits a bounded production panic record if the console is available.
#[doc(hidden)]
pub fn report_panic(location: Option<&core::panic::Location<'_>>) {
    logger::try_report_panic(location)
}

/// Emits the test-protocol panic terminal if the initialized console is free.
#[cfg(feature = "mtest")]
#[doc(hidden)]
pub fn report_mtest_panic() {
    mtest::report_panic()
}
