#![feature(strict_provenance)]
#![no_std]
#![no_main]

// TODO: RustSBI EFI module
use rcore_console::println;

mod drivers;
mod entry;
mod platform;

#[no_mangle]
extern "C" fn rust_main(_hart_id: usize, _opaque: usize) {
    platform::platform_init();

    println!("Hello world!");
    // TODO
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    // TODO panic handler
    loop {
        core::hint::spin_loop();
    }
}
