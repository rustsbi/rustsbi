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
mod trap;

struct Initialized {
    boot: machine::BootInfo,
    timer: Option<machine::Timer>,
    ipi: Option<machine::Ipi>,
    harts: Option<machine::HartControl>,
    fence: Option<machine::RemoteFence>,
    power: bool,
    console: Option<machine::Console>,
    memory: machine::memory::SupervisorMemory,
    counters: Option<machine::PerformanceCounters>,
    hart_count: usize,
}

fn initialize(mut boot: machine::BootInfo) -> Initialized {
    let facts = platform::discover(&*boot.dtb_mut()).unwrap_or_else(|_| fail());
    let (timer, ipi, fence, harts) =
        platform::install_timer_and_ipi(&mut boot, &facts).unwrap_or_else(|_| fail());

    let console = facts
        .console
        .as_ref()
        .map(|console| platform::install_console(console).unwrap_or_else(|_| fail()));
    if let Some(console) = console.as_ref() {
        logger::install(console.clone(), facts.harts.len()).unwrap_or_else(|_| fail());
    }
    let power = facts.power.as_ref().is_some_and(|power| {
        platform::install_power(power).unwrap_or_else(|_| fail());
        true
    });
    let protection = machine::pmp::config! {
        facts.memory.clone() => [read, write, execute];
    }
    .unwrap_or_else(|_| fail());
    boot.set_memory_protection(protection)
        .unwrap_or_else(|_| fail());
    let memory = boot.supervisor_memory().unwrap_or_else(|_| fail());
    let counters = boot
        .performance_counters()
        .map(Some)
        .unwrap_or_else(|_| fail());
    facts
        .prepare_supervisor_dtb(boot.dtb_mut())
        .unwrap_or_else(|_| fail());

    Initialized {
        boot,
        timer,
        ipi,
        harts,
        fence,
        power,
        console,
        memory,
        counters,
        hart_count: facts.harts.len(),
    }
}

/// Initializes firmware policy and enters the configured next stage.
pub fn run(boot: machine::BootInfo) -> ! {
    let Initialized {
        boot,
        timer,
        ipi,
        harts,
        fence,
        power,
        console,
        memory,
        counters,
        hart_count,
    } = initialize(boot);
    let sbi = sbi::Handler::new(
        timer, ipi, harts, fence, power, console, memory, counters, hart_count,
    );
    boot.enter_next_stage(Box::new(trap::Handler::new(sbi)))
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
