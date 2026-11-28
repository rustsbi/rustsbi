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
use riscv_csr_spec::standard::isa::values::status::Privilege;
use runtime::{
    PlatformDescription, Result, boot, entry,
    hart::{self, HartId},
    timer,
};

#[entry]
fn main(mut boot: BootInfo) {
    let result = if let Some(platform) = boot.take_platform_description() {
        boot_hart(boot, platform)
    } else {
        // SAFETY: Runtime entry supplies a boot handoff in M-mode with MPRV
        // clear and machine interrupts disabled, before lower-mode execution.
        // The trusted BSP confirms MXSTATUS/MAEE semantics on T-Head harts.
        unsafe { secondary_hart(Some(&boot)) }
    };
    if let Err(error) = result {
        error!("Failed to initialize the hart: {error}");
        fail::stop();
    }
}

fn boot_hart(boot: BootInfo, platform_description: PlatformDescription) -> Result<()> {
    // Publish the per-hart software records before platform discovery seeds
    // features and before secondary harts use any SBI service.
    sbi::features::init()?;
    sbi::pmu::init();
    sbi::rfence::init();
    // SAFETY:
    // 1. Runtime entry clears MPRV and masks interrupts in M-mode.
    //    Secondary harts wait for platform publication.
    // 2. The trusted boot handoff identifies the hardware and leaves cache/PMA
    //    setup to this hart with the loader's live mappings intact.
    // 3. On V821 the BSP reserves PMA entry 15. Published drivers use the
    //    custom CSRs in M-mode with Andes CCTL enabled and writeback-capable mappings.
    let next_stage_fdt_address = unsafe { firmware::init_platform(platform_description)? };

    // SAFETY: Runtime entry installed recovery and excluded interrupts. The
    // platform gives this hart its unlocked PMP entries before lower-mode entry.
    unsafe { firmware::set_pmp() };
    firmware::log_pmp_cfg();

    let hart = HartId::current()?;
    info!("{:<30}: {}", "Boot HART ID", hart.as_usize());

    // SAFETY: Boot entry installed recovery before any client could acquire an HPM counter.
    unsafe { detect_hart_features(hart)? };
    log_hart_capabilities(hart);

    let (next_mode, start_addr) = boot.next_stage();
    check_next_stage_privilege(hart, next_mode)?;
    info!(
        "Redirecting hart {} to {:#016x} in {:?} mode.",
        hart.as_usize(),
        start_addr,
        next_mode
    );
    hart::stage_boot_handoff(boot::NextStage {
        start_addr,
        next_mode,
        opaque: next_stage_fdt_address,
    })?;

    // SAFETY: Platform binding registered this hart's devices. Startup still owns
    // its supervisor state, and the BSP establishes the T-Head MAEE semantics.
    unsafe { enable_supervisor_services(hart) }
}

/// Initializes a secondary hart before its first lower-mode entry.
///
/// # Safety
///
/// 1. Enter in M-mode through Runtime startup or a supported reset vector with
///    Runtime recovery ready, `MPRV` clear and local machine interrupts disabled.
/// 2. Own this hart's startup configuration, including the unlocked PMP entries
///    needed for the published layout. The platform must meet the requirements of
///    [`enable_supervisor_services`].
unsafe fn secondary_hart(boot: Option<&BootInfo>) -> Result<()> {
    platform::wait_until_ready();
    let hart = HartId::current()?;
    if boot.is_none() {
        sbi::fwft::reset_hart(hart);
    }
    // SAFETY: Secondary startup owns this hart's counters with recovery active.
    unsafe { detect_hart_features(hart)? };

    // SAFETY: Platform publication is complete and the caller owns hart startup.
    unsafe { firmware::initialize_secondary_hart() };
    // SAFETY: The caller owns this hart's PMP setup with recovery ready and
    // interrupts excluded, before its first lower-mode access.
    unsafe { firmware::set_pmp() };

    // Hardware-reset harts have no SPL handoff; HSM starts them in S-mode.
    let next_mode = boot.map_or(Privilege::Supervisor, |boot| boot.next_stage().0);
    check_next_stage_privilege(hart, next_mode)?;

    // SAFETY: Device registration is complete and this hart still owns its
    // lower-mode configuration under the caller's startup contract.
    unsafe { enable_supervisor_services(hart) }
}

/// Configures supervisor services and clears the firmware timer deadline before handoff.
///
/// # Safety
///
/// Run on `hart` after platform device registration and before lower-mode execution.
/// Meet the hardware and startup requirements of [`sbi::features::configure_hart_environment`]
/// and [`boot::configure_next_stage`], including S-mode and the counter-enable CSRs.
unsafe fn enable_supervisor_services(hart: HartId) -> Result<()> {
    // SAFETY: The caller owns hart's lower-mode state and establishes the
    // board-specific CSR semantics required by configure_hart_environment.
    unsafe { sbi::features::configure_hart_environment(hart)? };
    // The supervisor owns its timer, so discard any firmware deadline before
    // entry; otherwise a stale comparator could inject an early interrupt.
    if let Some(timer) = timer::get()? {
        timer.set_deadline(u64::MAX)?;
    }
    // SAFETY:
    // 1. Runtime startup installed trap entry, cleared MPRV and set mscratch to zero.
    // 2. The startup contract supplies S-mode and the counter-enable CSRs.
    //    The registered devices and the current hart's interrupt file, if used,
    //    are ready for initialization before the first lower-mode context.
    // 3. Local machine interrupts remain disabled across setup.
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
