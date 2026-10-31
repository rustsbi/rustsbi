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

use crate::firmware::BootInfo;
use crate::sbi::features::{check_next_stage_privilege, counter_topology, detect_hart_features};
use ::riscv::register::mstatus::MPP;
use runtime::{
    boot, entry,
    hart::{self, HartId},
    ipi::IpiDelivery,
    trap,
};

#[entry]
fn main(boot: BootInfo) {
    if boot.is_boot_hart() {
        boot_hart(boot);
    } else {
        secondary_hart(Some(&boot));
    }
}

fn boot_hart(mut boot: BootInfo) {
    // Publish the per-hart software records before platform discovery seeds
    // features and before secondary harts use any SBI service.
    sbi::features::init();
    sbi::pmu::init();
    sbi::rfence::init();
    let platform_description = boot
        .take_platform_description()
        .expect("BUG: boot hart entered without a validated Platform Description");
    let next_stage_fdt_address = platform::init_board(platform_description);

    let firmware_ram = platform::firmware_ram_range();
    firmware::set_pmp(&firmware_ram);
    firmware::log_pmp_cfg(&firmware_ram);

    let hart = HartId::current().expect("BUG: current hart is not in the boot topology");
    info!("{:<30}: {}", "Boot HART ID", hart.as_usize());

    detect_hart_features();
    log_hart_capabilities(hart);

    let mut next_stage = boot.next_stage();
    check_next_stage_privilege(next_stage.next_mode);

    next_stage.opaque = next_stage_fdt_address;
    info!(
        "Redirecting hart {} to {:#016x} in {:?} mode.",
        hart.as_usize(),
        next_stage.start_addr,
        next_stage.next_mode
    );
    hart::stage_boot_handoff(next_stage)
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
    // Gate per-hart IMSIC setup on the device selected during platform
    // initialization, not on AIA discovery alone.
    if matches!(
        crate::platform::ipi_delivery(),
        Some(IpiDelivery::Imsic { .. })
    ) {
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
    // SAFETY:
    // 1. Runtime startup installed trap entry.
    // 2. This hart is still in M-mode with machine interrupts disabled and no
    //    lower-mode context.
    unsafe { boot::configure_next_stage() };
    let dispatcher = sbi::SBI_DISPATCHER
        .get()
        .expect("SBI dispatcher not initialized");
    trap::init(dispatcher).expect("BUG: failed to activate Runtime trap handling");
}

fn log_hart_capabilities(hart: HartId) {
    let (privileged_version, mhpm_mask) = counter_topology(hart);
    info!(
        "{:<30}: {:?}",
        "Boot HART Privileged Version:", privileged_version
    );
    info!("{:<30}: {:#08x}", "Boot HART MHPM Mask:", mhpm_mask);
}
