#![forbid(unsafe_code)]

use runtime::hart::HartId;

#[cfg(all(feature = "payload", feature = "jump"))]
compile_error!("feature \"payload\" and feature \"jump\" cannot be enabled at the same time");

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    let hart_id = HartId::current()
        .map(|hart| hart.as_usize())
        .unwrap_or(usize::MAX);
    let fault = runtime::trap::current_fault_snapshot();
    error!("Hart {} {info}", hart_id);
    error!("-----------------------------");
    error!("mcause:  {:?}", fault.mcause);
    error!("mepc:    {:#018x}", fault.mepc.bits());
    error!("mtval:   {:#018x}", fault.mtval.bits());
    error!("-----------------------------");
    error!("System shutdown scheduled due to RustSBI panic");
    loop {}
}

#[cold]
pub fn stop() -> ! {
    loop {
        core::hint::spin_loop()
    }
}
