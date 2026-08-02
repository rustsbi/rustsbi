#![feature(alloc_error_handler)]
#![no_std]
#![no_main]
#![forbid(unsafe_code)]

extern crate alloc;

use alloc::boxed::Box;
use core::alloc::Layout;
use core::panic::PanicInfo;

mod device_tree;
mod logger;
mod sbi;

/// Installs firmware policy and makes the one final next-stage transfer.
#[machine::entry]
fn main(mut boot: machine::BootInfo) -> ! {
    let mut tree = match device_tree::parse(&*boot.dtb_mut()) {
        Ok(tree) => tree,
        Err(_) => machine::abort(|| {}),
    };
    let supervisor_ram = match device_tree::memory(&tree) {
        Ok(memory) => memory,
        Err(_) => machine::abort(|| {}),
    };
    let discovered_harts = match device_tree::discover_harts(&tree) {
        Ok(harts) => harts,
        Err(_) => machine::abort(|| {}),
    };
    let hart_count = discovered_harts.len();
    if !boot.set_harts(&discovered_harts) {
        machine::abort(|| {});
    }

    let interrupts = match device_tree::install_interrupts(&mut boot, &mut tree, &discovered_harts)
    {
        Ok(interrupts) => interrupts,
        Err(_) => machine::abort(|| {}),
    };
    let console = match device_tree::install_console(&mut boot, &mut tree) {
        Ok(console) => console,
        Err(_) => machine::abort(|| {}),
    };
    if let Some(console) = console.as_ref()
        && !logger::install(console.clone(), hart_count)
    {
        machine::abort(|| {});
    }
    let power = match device_tree::install_power(&mut boot, &mut tree) {
        Ok(power) => power,
        Err(_) => machine::abort(|| {}),
    };

    let protection = match machine::pmp::config! {
        supervisor_ram.clone() => [read, write, execute];
    } {
        Ok(protection) => protection,
        Err(_) => machine::abort(|| {}),
    };
    if !boot.set_memory_protection(protection) {
        machine::abort(|| {});
    }
    let memory = match boot.supervisor_memory(core::slice::from_ref(&supervisor_ram)) {
        Ok(memory) => memory,
        Err(_) => machine::abort(|| {}),
    };
    let counters = match boot.performance_counters() {
        Ok(counters) => Some(counters),
        Err(_) => machine::abort(|| {}),
    };

    if device_tree::finish_device_tree(tree, boot.dtb_mut()).is_err() {
        machine::abort(|| {});
    }
    let dispatcher = match sbi::Dispatcher::from_capabilities(
        interrupts, power, console, memory, counters, hart_count,
    ) {
        Ok(dispatcher) => dispatcher,
        Err(_) => machine::abort(|| {}),
    };
    boot.enter_next_stage(Box::new(dispatcher))
}

#[panic_handler]
fn panic(info: &PanicInfo<'_>) -> ! {
    machine::abort(|| logger::report_panic(info.location()))
}

#[alloc_error_handler]
fn allocation_failure(_: Layout) -> ! {
    machine::abort(|| {})
}
