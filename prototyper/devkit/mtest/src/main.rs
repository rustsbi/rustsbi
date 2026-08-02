#![feature(alloc_error_handler)]
#![no_std]
#![no_main]
#![deny(unsafe_op_in_unsafe_fn)]

extern crate alloc;

use core::alloc::Layout;
use core::fmt::{self, Write};
use core::panic::PanicInfo;

use mtest_macros::mtest;
use sbi_testing::protocol::{self, Run};
use spin::Once;

mod registry;

pub use registry::Descriptor;

const QEMU_HARTS: [usize; 2] = [0, 1];
const QEMU_CLINT: core::ops::Range<usize> = 0x0200_0000..0x0201_0000;
const QEMU_UART: core::ops::Range<usize> = 0x1000_0000..0x1000_0100;
const QEMU_SIFIVE_TEST: core::ops::Range<usize> = 0x0010_0000..0x0010_1000;
const DEFAULT_DIGEST: &str = "0000000000000000000000000000000000000000000000000000000000000000";

static CONSOLE: Once<machine::Console> = Once::new();
static INTERRUPTS: Once<machine::Interrupts> = Once::new();

struct Output(&'static machine::Console);

impl Write for Output {
    fn write_str(&mut self, value: &str) -> fmt::Result {
        self.0
            .write_fmt(format_args!("{value}"))
            .map_err(|_| fmt::Error)
    }
}

#[machine::entry]
fn main(mut boot: machine::BootInfo) -> ! {
    if !boot.set_harts(&QEMU_HARTS) {
        abort();
    }
    let layout = match machine::interrupt::clint::Layout::new(
        QEMU_CLINT,
        QEMU_HARTS.to_vec(),
        machine::interrupt::clint::TimeSource::MemoryMapped,
    ) {
        Some(layout) => layout,
        None => abort(),
    };
    let interrupts = match machine::interrupt::clint::install(&mut boot, layout) {
        Some(interrupts) => interrupts,
        None => abort(),
    };
    INTERRUPTS.call_once(|| interrupts);

    let uart = match machine::IoMem::acquire(&mut boot, QEMU_UART) {
        Some(io) => match uart_16550::bind(io, uart_16550::Access::Byte) {
            Ok(console) => console,
            Err(_) => abort(),
        },
        None => abort(),
    };
    CONSOLE.call_once(|| uart);

    let test = match machine::IoMem::acquire(&mut boot, QEMU_SIFIVE_TEST) {
        Some(io) => io,
        None => abort(),
    };
    if !sifive_test::bind(test) {
        abort();
    }

    run_selected_case()
}

fn run_selected_case() -> ! {
    let metadata = run_metadata();
    let mut output = Output(console());
    let registry = match registry::from_linker_bounds() {
        Ok(registry) => registry,
        Err(_) => {
            let _ = protocol::harness_fail(&mut output, metadata, "REGISTRY_FAILURE");
            terminate(machine::power::PowerReason::SystemFailure)
        }
    };
    if option_env!("RUSTSBI_MTEST_LIST").is_some() {
        let mut failed = false;
        registry.visit(|name| {
            failed |= writeln!(output, "@@RUSTSBI_MTEST type=CASE name={name}").is_err();
        });
        terminate(if failed {
            machine::power::PowerReason::SystemFailure
        } else {
            machine::power::PowerReason::Unspecified
        })
    }

    let filter = option_env!("RUSTSBI_MTEST_FILTER").unwrap_or("");
    let Some(test) = registry.select(filter) else {
        let _ = protocol::harness_fail(&mut output, metadata, "TEST_NOT_FOUND");
        terminate(machine::power::PowerReason::SystemFailure)
    };
    if protocol::run_start(&mut output, metadata, 1).is_err()
        || protocol::case_start(&mut output, metadata, test.name()).is_err()
    {
        terminate(machine::power::PowerReason::SystemFailure)
    }
    let name = test.name();
    test.run();
    if protocol::case_pass(&mut output, metadata, name).is_err()
        || protocol::run_pass(&mut output, metadata, 1).is_err()
    {
        terminate(machine::power::PowerReason::SystemFailure)
    }
    terminate(machine::power::PowerReason::Unspecified)
}

fn run_metadata() -> Run<'static> {
    Run {
        shard: option_env!("RUSTSBI_TEST_SHARD").unwrap_or("mtest"),
        run: option_env!("RUSTSBI_TEST_RUN_ID").unwrap_or("mtest"),
        attempt: 1,
        seed: 0,
        digest: option_env!("RUSTSBI_TEST_DIGEST").unwrap_or(DEFAULT_DIGEST),
    }
}

fn console() -> &'static machine::Console {
    match CONSOLE.get() {
        Some(console) => console,
        None => abort(),
    }
}

fn interrupts() -> &'static machine::Interrupts {
    match INTERRUPTS.get() {
        Some(interrupts) => interrupts,
        None => abort(),
    }
}

#[mtest]
fn qemu_virt_console_is_bound() {
    console()
        .write_fmt(format_args!("@@RUSTSBI_MTEST type=CONSOLE_PROBE\n"))
        .expect("the fixed QEMU UART must accept a bounded record");
}

#[mtest]
fn qemu_virt_clint_accepts_a_deadline() {
    interrupts().timer.set_deadline(u64::MAX);
}

#[panic_handler]
fn panic(_: &PanicInfo<'_>) -> ! {
    let mut output = Output(console());
    let metadata = run_metadata();
    let _ = protocol::harness_fail(&mut output, metadata, "UNEXPECTED_PANIC");
    terminate(machine::power::PowerReason::SystemFailure)
}

#[alloc_error_handler]
fn allocation_failure(_: Layout) -> ! {
    abort()
}

fn terminate(reason: machine::power::PowerReason) -> ! {
    let _ = machine::power::shutdown(reason);
    abort()
}

fn abort() -> ! {
    machine::abort(|| {})
}
