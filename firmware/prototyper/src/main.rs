#![feature(alloc_error_handler)]
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
mod heap;
mod platform;
mod riscv;
mod sbi;

use crate::driver::{ipi, timer};
use crate::firmware::BootInfo;
use crate::sbi::features::{
    check_next_stage_privilege, detect_hart_features, hart_mhpm_mask, hart_privileged_version,
};
use crate::sbi::hart_local;
use ::riscv::register::mstatus::MPP;
use runtime::hart::HartId;
use rustsbi_prototyper_macros::entry;

#[entry]
fn main(boot: BootInfo) {
    if boot.is_boot_hart() {
        boot_hart(boot);
    } else {
        secondary_hart(Some(&boot));
    }
}

fn boot_hart(mut boot: BootInfo) {
    heap::init();
    // Initialize this hart's policy storage before any user: platform
    // discovery seeds secondary harts' features, and feature detection
    // writes this hart's.
    hart_local::init();
    let platform_description = boot
        .take_platform_description()
        .expect("BUG: boot hart entered without a validated Platform Description");
    let next_stage_fdt_address = platform::init_board(platform_description);

    let firmware_ram = platform::firmware_ram_range();
    firmware::set_pmp(&firmware_ram);
    firmware::log_pmp_cfg(&firmware_ram);

    let hart_id = HartId::current()
        .expect("BUG: current hart exceeds Runtime capacity")
        .as_usize();
    info!("{:<30}: {}", "Boot HART ID", hart_id);

    detect_hart_features();
    log_hart_capabilities(hart_id);

    let mut next_stage = boot.next_stage();
    check_next_stage_privilege(next_stage.next_mode);

    next_stage.opaque = next_stage_fdt_address;
    info!(
        "Redirecting hart {} to {:#016x} in {:?} mode.",
        hart_id, next_stage.start_addr, next_stage.next_mode
    );
    runtime::hart::stage_current(next_stage)
        .expect("BUG: boot hart could not stage its initial handoff");

    enable_supervisor_services();
}

fn secondary_hart(boot: Option<&BootInfo>) {
    platform::wait_until_ready();
    detect_hart_features();

    platform::initialize_secondary_hart();
    firmware::set_pmp(&platform::firmware_ram_range());

    // Hardware-reset harts have no SPL handoff; HSM starts them in S-mode.
    let next_mode = boot.map_or(MPP::Supervisor, |boot| boot.next_stage().next_mode);
    check_next_stage_privilege(next_mode);

    enable_supervisor_services();
}

fn enable_supervisor_services() {
    ipi::clear_current();
    timer::clear_current();
    // Gate per-hart IMSIC setup on the device selected during platform
    // initialization, not on AIA discovery alone.
    if ipi::uses_imsic() {
        driver::initialize_hart_imsic(
            platform::board_info()
                .devices
                .interrupts
                .imsic()
                .map(|description| description.resource())
                .expect("selected IMSIC has a description"),
        );
    }
    sbi::features::configure_hart_environment();
    // Transactional per-hart trap activation: publishes the policy, applies
    // the fixed delegation/counter policy, and installs the final trap
    // vector as the Ready commit point.
    runtime::trap::init(
        sbi::SBI_DISPATCHER
            .get()
            .expect("SBI dispatcher not initialized"),
    )
    .expect("BUG: failed to activate Runtime trap handling");
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
