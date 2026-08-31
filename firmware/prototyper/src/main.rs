#![feature(alloc_error_handler)]
#![feature(fn_align)]
#![no_std]
#![no_main]

extern crate alloc;
#[macro_use]
extern crate log;

mod cfg;
mod devicetree;
mod driver;
mod fail;
mod firmware;
mod platform;
mod riscv;
mod sbi;

use crate::firmware::BootInfo;
use crate::riscv::current_hartid;
use crate::sbi::features::{
    check_privilege, detect_hart_features, hart_mhpm_mask, hart_privileged_version,
};
use crate::sbi::heap;
use crate::sbi::hsm::hart_hsm;
use crate::sbi::trap_stack;
use crate::sbi::{ipi, timer};
use rustsbi_prototyper_macros::entry;

#[entry]
fn main(boot: BootInfo) {
    if boot.is_boot_hart() {
        boot_hart(&boot);
    } else {
        secondary_hart(&boot);
    }
}

fn boot_hart(boot: &BootInfo) {
    heap::init();
    platform::init_board(boot.fdt_address());

    let mem = platform::memory_range();
    firmware::set_pmp(&mem);
    firmware::log_pmp_cfg(&mem);

    let hart_id = current_hartid();
    info!("{:<30}: {}", "Boot HART ID", hart_id);

    detect_hart_features();
    trap_stack::prepare_for_trap();
    log_hart_capabilities(hart_id);

    let mut next = boot.next_stage();
    check_privilege(next.next_mode);

    platform::refresh_cpu_features();
    next.opaque = firmware::patch_device_tree(boot.fdt_address());
    info!(
        "Redirecting hart {} to {:#016x} in {:?} mode.",
        hart_id, next.start_addr, next.next_mode
    );
    hart_hsm().start(next);

    enable_supervisor_services();
}

fn secondary_hart(boot: &BootInfo) {
    detect_hart_features();
    trap_stack::prepare_for_trap();

    platform::wait_until_ready();
    platform::secondary_hart_init();
    firmware::set_pmp(&platform::memory_range());

    let next = boot.next_stage();
    check_privilege(next.next_mode);

    enable_supervisor_services();
}

fn enable_supervisor_services() {
    ipi::claim_ipi();
    timer::clear();
    // Gate per-hart IMSIC setup on the device selected during platform
    // initialization, not on AIA discovery alone.
    if ipi::uses_imsic() {
        driver::per_hart_init();
    }
    sbi::features::configure_delegation_and_trap();
}

fn log_hart_capabilities(hart_id: usize) {
    info!(
        "{:<30}: {:?}",
        "Boot HART Privileged Version:",
        hart_privileged_version(hart_id)
    );
    info!(
        "{:<30}: {:#08x}",
        "Boot HART MHPM Mask:",
        hart_mhpm_mask(hart_id)
    );
}
