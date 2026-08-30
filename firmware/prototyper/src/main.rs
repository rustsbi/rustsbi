#![feature(alloc_error_handler)]
#![feature(fn_align)]
#![no_std]
#![no_main]

extern crate alloc;
#[macro_use]
extern crate log;

mod cfg;
mod devicetree;
mod fail;
mod firmware;
mod platform;
mod riscv;
mod rpmi;
mod sbi;

use crate::firmware::BootInfo;
use crate::riscv::current_hartid;
use crate::sbi::features::{
    check_privilege, detect_hart_features, hart_mhpm_mask, hart_privileged_version,
};
use crate::sbi::heap;
use crate::sbi::hsm::hart_hsm;
use crate::sbi::ipi;
use crate::sbi::trap_stack;
use ::riscv::register::mstatus::MPP;
use rustsbi_prototyper_macros::entry;

#[entry]
fn main(boot: BootInfo) {
    // A K3 hart started through HSM has no dynamic boot envelope. Its pending
    // HSM state determines that it must follow the secondary path.
    let woken = crate::sbi::hsm::local_hsm().has_pending();
    if boot.is_boot_hart() && !woken {
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
    // Secondary harts are parked by SPL and never enter RustSBI to
    // self-initialize; clear their HSM cells so HSM `hart_start` finds every
    // hart in STOPPED and can wake it for smp bring-up.
    crate::sbi::hsm::init_secondary_hsm_cells(hart_id);

    enable_supervisor_services();
}

fn secondary_hart(boot: &BootInfo) {
    // Preserve START_PENDING across hart-local initialization.
    let pending = crate::sbi::hsm::local_hsm().take_pending();
    let woken = pending.is_some();

    detect_hart_features();
    trap_stack::prepare_for_trap();

    if let Some(next) = pending {
        crate::sbi::hsm::local_hsm().restore_pending(next);
    }

    // Enable this hart's core snoop BEFORE spinning on the boot-ready flag,
    // so a cold-booted secondary observes the boot hart's writes (READY,
    // delegate/trap setup) coherently.
    platform::secondary_hart_init();
    platform::wait_until_ready();
    platform::secondary_hart_init();
    firmware::set_pmp(&platform::memory_range());

    if woken {
        // The HSM start payload, not DynamicInfo, defines the next stage.
        check_privilege(MPP::Supervisor);
    } else {
        let next = boot.next_stage();
        check_privilege(next.next_mode);
    }

    // Publish the privilege check before HSM validates future starts.
    platform::refresh_cpu_features();

    enable_supervisor_services();
}

fn enable_supervisor_services() {
    ipi::clear_all();
    platform::aia::per_hart_init();
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
