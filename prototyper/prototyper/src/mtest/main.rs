#![feature(alloc_error_handler)]
#![no_std]
#![no_main]
#![forbid(unsafe_code)]

use core::alloc::Layout;
use core::panic::PanicInfo;

machine::entry!(main);

fn main(boot: machine::BootInfo) -> ! {
    rustsbi_prototyper::run_mtest(boot)
}

#[panic_handler]
fn panic(_: &PanicInfo<'_>) -> ! {
    machine::abort(rustsbi_prototyper::report_mtest_panic)
}

#[alloc_error_handler]
fn allocation_failure(_: Layout) -> ! {
    machine::abort(|| {})
}
