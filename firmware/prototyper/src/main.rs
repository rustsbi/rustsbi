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
use runtime::{
    DeviceTreeHandoff,
    boot::NextStage,
    entry,
    hart::{Hart, TargetPrivilege},
    trap,
};

#[entry(initialize = firmware::initialize, stack_size = cfg::STACK_SIZE_PER_HART, link_start = cfg::SBI_LINK_START_ADDRESS)]
fn main(hart_id: usize, device_tree: DeviceTreeHandoff, dynamic_info: usize) {
    let boot = firmware::boot_info(hart_id, device_tree, dynamic_info);
    if boot.is_boot_hart() {
        boot_hart(boot);
    } else {
        secondary_hart(Some(&boot));
    }
}

fn boot_hart(mut boot: BootInfo) {
    let platform_description = boot
        .take_platform_description()
        .expect("BUG: boot hart entered without a validated Platform Description");
    let next_stage_fdt_address = platform::init_board(platform_description);

    let firmware_ram = platform::firmware_ram_range();
    firmware::set_pmp(&firmware_ram);
    firmware::log_pmp_cfg(&firmware_ram);

    let hart = Hart::current().expect("current hart description");
    let hart_id = hart.id().as_usize();
    info!("{:<30}: {}", "Boot HART ID", hart_id);

    let next_stage = boot.next_stage();
    hart.initialize(next_stage.next_mode())
        .expect("failed to initialize hart");
    if matches!(
        next_stage.next_mode(),
        TargetPrivilege::Supervisor | TargetPrivilege::User
    ) {
        platform::mark_hart_privilege_checked(hart_id);
    }
    log_hart_capabilities(hart);

    // SAFETY: The loader supplies an architectural entry. Platform setup
    // produced an exclusive FDT handoff that remains live for the next stage.
    let next_stage = unsafe {
        NextStage::new(
            next_stage.start_addr(),
            next_stage_fdt_address,
            next_stage.next_mode(),
        )
    };
    info!(
        "Redirecting hart {} to {:#016x} in {:?} mode.",
        hart_id,
        next_stage.start_addr(),
        next_stage.next_mode()
    );
    hart.stage_boot_handoff(next_stage)
        .expect("BUG: boot hart could not stage its initial handoff");

    let dispatcher = sbi::SBI_DISPATCHER
        .get()
        .expect("SBI dispatcher not initialized");
    trap::init(dispatcher).expect("failed to publish SBI policy");
}

fn secondary_hart(boot: Option<&BootInfo>) {
    platform::wait_until_ready();

    platform::initialize_secondary_hart();
    firmware::set_pmp(&platform::firmware_ram_range());

    // Hardware-reset harts have no SPL handoff; HSM starts them in S-mode.
    let next_mode = boot.map_or(TargetPrivilege::Supervisor, |boot| {
        boot.next_stage().next_mode()
    });
    let hart = Hart::current().expect("current hart description");
    hart.initialize(next_mode)
        .expect("failed to initialize hart");
    if matches!(
        next_mode,
        TargetPrivilege::Supervisor | TargetPrivilege::User
    ) {
        platform::mark_hart_privilege_checked(hart.id().as_usize());
    }
    let dispatcher = sbi::SBI_DISPATCHER
        .get()
        .expect("SBI dispatcher not initialized");
    trap::init(dispatcher).expect("failed to publish SBI policy");
}

fn log_hart_capabilities(hart: &Hart) {
    info!(
        "{:<30}: {:?}",
        "Boot HART Privileged Version:",
        hart.privileged_version()
    );
    let mask = runtime::pmu::Pmu::with_current(|counters| Ok(counters.implemented()))
        .expect("initialized PMU counters");
    info!("{:<30}: {:#08x}", "Boot HART MHPM Mask:", mask);
}
