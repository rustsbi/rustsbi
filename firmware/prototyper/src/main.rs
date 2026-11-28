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
    // 1. Only the elected boot hart receives the platform description. It
    //    calls this once, with MPRV clear and interrupts masked in M-mode.
    //    Secondary harts wait for platform publication.
    // 2. The trusted boot handoff identifies the hardware and leaves cache/PMA
    //    setup to this hart with the loader's live mappings intact.
    // 3. On V821 the BSP assigns PMA entry 15 and its configuration word to
    //    firmware. It enables CCTL for physical-address commands in M-mode, and
    //    the BSP permits whole-cache writeback while device buffers are live.
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
    // SAFETY: The boot hart uses the target authorized by the loader or linked
    // image. The dynamic handoff requires an M-mode payload to preserve live firmware memory.
    unsafe { hart::stage_boot_handoff(next_mode, start_addr, next_stage_fdt_address)? };

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
/// 2. Own this hart's startup configuration and satisfy [`firmware::set_pmp`]
///    and [`enable_supervisor_services`] for this hart.
/// 3. Pass the loader handoff on initial entry. Pass `None` only after a
///    hardware reset whose entry code has prepared the hart's caches for shared access.
unsafe fn secondary_hart(boot: Option<&BootInfo>) -> Result<()> {
    platform::wait_until_ready();
    let hart = HartId::current()?;
    if boot.is_none() {
        sbi::fwft::reset_hart(hart);
    }
    // SAFETY: Secondary startup owns this hart's counters with recovery active.
    unsafe { detect_hart_features(hart)? };

    if boot.is_some() {
        // SAFETY: On loader entry, startup still owns this hart's cache setup.
        // The acquire in wait_until_ready observed the boot hart's publication.
        unsafe { firmware::initialize_secondary_hart() };
    }
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
    // SAFETY: The caller authorizes the supervisor state grants and establishes
    // the board-specific CSR semantics before lower-mode entry.
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
