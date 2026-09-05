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
    check_next_stage_privilege, detect_hart_features, hart_mhpm_mask, hart_privileged_version,
};
use crate::sbi::heap;
use crate::sbi::hsm::hart_hsm;
use crate::sbi::trap_stack;
use crate::sbi::{ipi, timer};
use rustsbi_prototyper_macros::entry;

#[entry]
fn main(boot: BootInfo) {
    if boot.is_boot_hart() {
        boot_hart(boot);
    } else {
        secondary_hart(&boot);
    }
}

fn boot_hart(mut boot: BootInfo) {
    heap::init();
    let platform_description = boot
        .take_platform_description()
        .expect("BUG: boot hart entered without a validated Platform Description");
    let next_stage_fdt_address = platform::init_board(platform_description);

    let firmware_ram = platform::firmware_ram_range();
    firmware::set_pmp(&firmware_ram);
    firmware::log_pmp_cfg(&firmware_ram);

    let hart_id = current_hartid();
    info!("{:<30}: {}", "Boot HART ID", hart_id);

    detect_hart_features();
    trap_stack::prepare_for_trap();
    log_hart_capabilities(hart_id);

    let mut next_stage = boot.next_stage();
    check_next_stage_privilege(next_stage.next_mode);

    platform::retain_privilege_checked_harts();
    next_stage.opaque = next_stage_fdt_address;
    info!(
        "Redirecting hart {} to {:#016x} in {:?} mode.",
        hart_id, next_stage.start_addr, next_stage.next_mode
    );
    hart_hsm().start(next_stage);

    enable_supervisor_services();
}

fn secondary_hart(boot: &BootInfo) {
    detect_hart_features();
    trap_stack::prepare_for_trap();

    platform::wait_until_ready();
    platform::initialize_secondary_hart();
    firmware::set_pmp(&platform::firmware_ram_range());

    let next_stage = boot.next_stage();
    check_next_stage_privilege(next_stage.next_mode);

    enable_supervisor_services();
}

fn enable_supervisor_services() {
    ipi::claim_ipi();
    timer::clear();
    // Gate per-hart IMSIC setup on the device selected during platform
    // initialization, not on AIA discovery alone.
    if ipi::uses_imsic() {
        driver::initialize_hart_imsic();
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
