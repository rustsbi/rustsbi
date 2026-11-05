#![feature(alloc_error_handler)]
#![feature(array_try_from_fn)]
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
mod sbi;

use crate::driver::platform;
use crate::firmware::BootInfo;
use crate::sbi::features::{check_next_stage_privilege, counter_topology, detect_hart_features};
use csr::machine::TargetPrivilege as MPP;
use runtime::{
    Error, Result, boot, entry,
    hart::{self, HartId},
    timer,
};

#[entry]
fn main(boot: BootInfo) {
    let result = if boot.is_boot_hart() {
        boot_hart(boot)
    } else {
        secondary_hart(Some(&boot))
    };
    if let Err(error) = result {
        error!("Failed to initialize the hart: {error}");
        fail::stop();
    }
}

fn boot_hart(mut boot: BootInfo) -> Result<()> {
    // Publish the per-hart software records before platform discovery seeds
    // features and before secondary harts use any SBI service.
    sbi::features::init()?;
    sbi::pmu::init();
    sbi::rfence::init();
    let platform_description = boot.take_platform_description().ok_or(Error::NotReady)?;
    let next_stage_fdt_address = platform::init_platform(platform_description)?;

    firmware::set_pmp();
    firmware::log_pmp_cfg();

    let hart = HartId::current()?;
    info!("{:<30}: {}", "Boot HART ID", hart.as_usize());

    detect_hart_features(hart);
    log_hart_capabilities(hart);

    let mut next_stage = boot.next_stage();
    check_next_stage_privilege(hart, next_stage.next_mode)?;

    next_stage.opaque = next_stage_fdt_address;
    info!(
        "Redirecting hart {} to {:#016x} in {:?} mode.",
        hart.as_usize(),
        next_stage.start_addr,
        next_stage.next_mode
    );
    hart::stage_boot_handoff(next_stage)?;

    enable_supervisor_services(hart)
}

fn secondary_hart(boot: Option<&BootInfo>) -> Result<()> {
    platform::wait_until_ready();
    let hart = HartId::current()?;
    detect_hart_features(hart);

    platform::initialize_secondary_hart(hart);
    firmware::set_pmp();

    // Hardware-reset harts have no SPL handoff; HSM starts them in S-mode.
    let next_mode = boot.map_or(MPP::Supervisor, |boot| boot.next_stage().next_mode);
    check_next_stage_privilege(hart, next_mode)?;

    enable_supervisor_services(hart)
}

fn enable_supervisor_services(hart: HartId) -> Result<()> {
    sbi::features::configure_hart_environment(hart)?;
    // The supervisor owns its timer, so discard any firmware deadline before
    // entry; otherwise a stale comparator could inject an early interrupt.
    if let Some(timer) = timer::get() {
        timer.set_deadline(u64::MAX);
    }
    // SAFETY:
    // 1. Runtime startup installed trap entry.
    // 2. This hart is still in M-mode with machine interrupts disabled and no
    //    lower-mode context.
    unsafe { boot::configure_next_stage() };
    Ok(())
}

fn log_hart_capabilities(hart: HartId) {
    let (privileged_version, mhpm_mask) = counter_topology(hart);
    info!(
        "{:<30}: {:?}",
        "Boot HART Privileged Version:", privileged_version
    );
    info!("{:<30}: {:#08x}", "Boot HART MHPM Mask:", mhpm_mask);
}
