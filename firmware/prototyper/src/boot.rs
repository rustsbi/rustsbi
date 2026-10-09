//! Prototyper policy for cold boot and secondary-hart initialization.
//!
//! Runtime owns entry, handoff reads and stack publication. This module selects
//! the boot candidate, prepares owned policy data and activates platform services.

use riscv::register::mstatus::MPP;
use runtime::boot::{BootInput, BootPolicy, BootStorage, FirmwareEntry, NextStage, PreparedBoot};
use runtime::hart::HartId;
use runtime::memory::PhysAddr;

use crate::platform::protection;
use crate::sbi::{features, hart_local};
use crate::{cfg, fail, heap, next_stage, platform, sbi};

struct PrototyperBoot;

type BootContext = Result<NextStage, next_stage::Error>;

static BOOT_STORAGE: BootStorage<BootContext> = BootStorage::new();

impl BootPolicy for PrototyperBoot {
    type Context = BootContext;
    type PrepareError = runtime::Error;
    const STORAGE: &'static BootStorage<BootContext> = &BOOT_STORAGE;
    const HART_CAPACITY: usize = cfg::HART_CAPACITY;
    const STACK_SIZE_PER_HART: usize = cfg::STACK_SIZE_PER_HART;
    const USE_DYNAMIC_HANDOFF: bool = !cfg!(any(feature = "payload", feature = "jump"));

    fn prepare(input: BootInput) -> runtime::Result<Option<PreparedBoot<BootContext>>> {
        let designated_hart = input
            .dynamic_info
            .as_ref()
            .and_then(|snapshot| snapshot.as_ref().ok())
            .map(|info| info.boot_hart)
            .filter(|hart| *hart != usize::MAX);
        if designated_hart.is_some_and(|hart| hart != input.hart_id) {
            return Ok(None);
        }
        let device_tree_address = selected_device_tree(&input.device_tree);
        let platform = input.device_tree.claim(device_tree_address)?;
        if designated_hart.is_none()
            && !platform.inspect(|view| {
                for hart in view.hart_ids()? {
                    if hart? == input.hart_id {
                        return Ok(true);
                    }
                }
                Ok(false)
            })?
        {
            return Ok(None);
        }
        let selection = next_stage::select(input.dynamic_info);
        heap::init();
        Ok(Some(PreparedBoot {
            platform,
            stack_exclusion_entry: selection.stack_exclusion_entry,
            context: selection.next_stage,
        }))
    }

    fn initialize_boot(boot: PreparedBoot<BootContext>) {
        initialize_boot_hart(boot);
    }

    fn initialize_secondary(input: BootInput) {
        let next_stage = next_stage::select(input.dynamic_info).next_stage;
        initialize_secondary_hart(Some(next_stage));
    }
}

#[used]
static FIRMWARE_ENTRY: FirmwareEntry = FirmwareEntry::new::<PrototyperBoot>();

fn selected_device_tree(handoff: &runtime::DeviceTreeHandoff) -> PhysAddr {
    #[cfg(feature = "fdt")]
    {
        let _ = handoff;
        runtime::boot::embedded_fdt()
            .expect("BUG: fdt firmware has no embedded device tree")
            .start()
    }
    #[cfg(not(feature = "fdt"))]
    handoff.address()
}

fn initialize_boot_hart(boot: PreparedBoot<BootContext>) {
    // Platform discovery seeds all policy slots before this hart detects features.
    hart_local::init();
    let next_stage_fdt_address = platform::init_board(boot.platform);
    let pmp_entries = protection::install(&platform::firmware_ram_range());
    protection::log(pmp_entries);

    let hart_id = HartId::current()
        .expect("BUG: current hart is outside the published boot topology")
        .as_usize();
    info!("{:<30}: {}", "Boot HART ID", hart_id);
    detect_current_hart();
    info!(
        "{:<30}: {:?}",
        "Boot HART Privileged Version:",
        features::hart_privileged_version(hart_id)
    );
    info!(
        "{:<30}: {:#08x}",
        "Boot HART MHPM Mask:",
        runtime::pmu::Pmu::current()
            .and_then(|pmu| pmu.probe())
            .expect("failed to discover current-hart counters")
            .mask()
    );

    let mut next_stage = boot.context.unwrap_or_else(|error| fail::next_stage(error));
    features::check_next_stage_privilege(next_stage.next_mode);
    next_stage.opaque = next_stage_fdt_address;
    info!(
        "Redirecting hart {} to {:#016x} in {:?} mode.",
        hart_id, next_stage.start_addr, next_stage.next_mode
    );
    runtime::hart::stage_current(next_stage)
        .expect("BUG: boot hart could not stage its initial handoff");
    enable_supervisor_services();
}

/// Initializes a hardware-reset hart after Runtime prepares its stack.
pub(crate) fn initialize_reset_hart() {
    initialize_secondary_hart(None);
}

fn initialize_secondary_hart(next_stage: Option<Result<NextStage, next_stage::Error>>) {
    platform::wait_until_ready();
    detect_current_hart();
    platform::initialize_secondary_hart();
    protection::install(&platform::firmware_ram_range());
    // Loader-started harts validate the complete handoff before using its mode.
    // Hardware-reset harts have no loader handoff and HSM starts them in S-mode.
    let mode = next_stage.map_or(MPP::Supervisor, |stage| {
        stage
            .unwrap_or_else(|error| fail::next_stage(error))
            .next_mode
    });
    features::check_next_stage_privilege(mode);
    enable_supervisor_services();
}

fn detect_current_hart() {
    features::detect_hart_features().unwrap_or_else(|error| fail::hart_initialization(error));
}

fn enable_supervisor_services() {
    features::configure_hart_environment().unwrap_or_else(|error| fail::hart_initialization(error));
    runtime::trap::init(
        sbi::SBI_DISPATCHER
            .get()
            .expect("SBI dispatcher not initialized"),
    )
    .expect("BUG: failed to activate Runtime trap handling");
}
