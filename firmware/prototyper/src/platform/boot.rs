#![forbid(unsafe_code)]

//! One-time platform discovery, resource acquisition, and publication.

use alloc::boxed::Box;
use core::ops::Range;

use runtime::memory::SupervisorMemory;
use spin::Mutex;

use super::error::{self, ResultContext};
use super::info::BoardInfo;
use super::{discovery, report, state};
use crate::driver::{self, InterruptDevices, ResetDevice};
use crate::riscv::spacemit_k1::{self, K1BootResources};
use crate::sbi;
use crate::sbi::SbiDispatcher;
use crate::sbi::cppc::SbiCppc;
use crate::sbi::dbtr::SbiDbtr;
use crate::sbi::fwft::SbiFwft;
use crate::sbi::hsm::SbiHsm;
use crate::sbi::nacl::SbiNacl;
use crate::sbi::pmu::SbiPmu;
use crate::sbi::reset::SbiReset;
use crate::sbi::rfence::SbiRFence;
use crate::sbi::sse::SbiSse;
use crate::sbi::sta::SbiSta;
use crate::sbi::suspend::SbiSuspend;

/// Discovers the platform, initializes its devices, and publishes its
/// services. Returns the device tree prepared for the next stage.
pub fn init_board(platform_description: runtime::PlatformDescription) -> usize {
    try_init_board(platform_description).unwrap_or_else(|error| panic!("{error}"))
}

fn try_init_board(mut platform_description: runtime::PlatformDescription) -> error::Result<usize> {
    let device_tree_address = platform_description.address().as_usize();
    let (mut board, pmu) = platform_description
        .inspect(discover_board_and_pmu)
        .during("reading the platform description")?;

    let (supervisor_memory, mut memory) = platform_description
        .into_memory_resources()
        .during("deriving Runtime memory resources")?;
    board.ram_ranges = memory.ram_ranges().collect();
    let firmware_image_range = memory.firmware_image_range();
    board.firmware_ram_range = Some(
        board
            .ram_range_containing(firmware_image_range)
            .ok_or(runtime::Error::InvalidArgs)
            .during("locating the firmware RAM bank")?,
    );

    let devices = driver::bind_devices(&board, &mut memory).during("binding platform devices")?;
    let k1_resources = board
        .spacemit_k1
        .map(|registers| K1BootResources::acquire(&mut memory, registers))
        .transpose()
        .during("acquiring SpacemiT K1 resources")?;
    let is_k1 = k1_resources.is_some();

    let uses_imsic = devices.uses_imsic();
    let next_stage_fdt_address = crate::firmware::patch_device_tree(
        device_tree_address,
        &board,
        firmware_image_range,
        uses_imsic,
    )
    .during("preparing the next-stage platform description")?;

    if let Some(k1_resources) = k1_resources {
        spacemit_k1::initialize_boot_hart(k1_resources);
    }

    publish_platform_services(board, supervisor_memory, devices, pmu, is_k1);
    Ok(next_stage_fdt_address)
}

fn discover_board_and_pmu(
    platform: runtime::PlatformView<'_>,
) -> runtime::Result<(BoardInfo, Option<SbiPmu>)> {
    let board = discovery::discover_platform(&platform)?;
    let pmu = sbi::pmu::init(platform.root());
    Ok((board, pmu))
}

fn publish_platform_services(
    board: BoardInfo,
    supervisor_memory: SupervisorMemory,
    devices: driver::Devices,
    pmu: Option<SbiPmu>,
    is_k1: bool,
) {
    let driver::Devices {
        interrupts,
        console,
        reset,
    } = devices;
    state::publish_resources(board, supervisor_memory, console);

    sbi::logger::Logger::init().expect("BUG: firmware logger initialized more than once");
    info!("Hello RustSBI!");

    publish_sbi_dispatcher(interrupts, reset, pmu);

    state::mark_ready();

    report::log_platform_summary();
    if is_k1 {
        info!("SpacemiT K1: early init done (MSETUP + CCI-550)");
    }
}

fn publish_sbi_dispatcher(
    interrupts: Option<InterruptDevices>,
    reset: Option<Box<dyn ResetDevice>>,
    pmu: Option<SbiPmu>,
) {
    let supervisor_memory = state::supervisor_memory();
    let console = state::console_device()
        .map(|device| sbi::console::SbiConsole::new(device, supervisor_memory));
    let cppc = Some(SbiCppc::new());
    let dbtr = Some(SbiDbtr::new(supervisor_memory));
    let fwft = Some(SbiFwft);
    let (ipi, timer) = match interrupts {
        Some(devices) => (
            Some(sbi::ipi::init(devices.ipi)),
            Some(sbi::timer::SbiTimer::new(devices.timer)),
        ),
        None => (None, None),
    };
    let hsm = ipi.as_ref().map(|_| SbiHsm);
    let reset = reset.map(|device| SbiReset::new(Mutex::new(device)));
    let rfence = ipi.as_ref().map(|_| SbiRFence);
    let susp = hsm.as_ref().map(|_| SbiSuspend);
    let mpxy = Some(sbi::mpxy::SbiMpxy::new(supervisor_memory));
    let sta = Some(SbiSta::new(supervisor_memory));
    let nacl = Some(SbiNacl::new(supervisor_memory));
    // Keep SSE unavailable until supervisor handler context switching is implemented.
    let sse: Option<SbiSse> = None;

    sbi::SBI_DISPATCHER.call_once(|| {
        SbiDispatcher::new(
            console, cppc, dbtr, fwft, ipi, timer, hsm, reset, rfence, susp, pmu, sta, mpxy, nacl,
            sse,
        )
    });
}

/// Runs the SoC-specific per-hart setup for secondary harts.
pub fn initialize_secondary_hart() {
    if let Some(platform) = state::board_info().spacemit_k1 {
        spacemit_k1::initialize_hart(platform);
    }
}

/// Spins until the boot hart has finished platform initialization.
pub fn wait_until_ready() {
    state::wait_until_ready()
}

/// Returns the RAM bank containing the linked firmware image.
pub fn firmware_ram_range() -> Range<usize> {
    let range = state::board_info()
        .firmware_ram_range
        .expect("BUG: firmware RAM bank missing after platform initialization");
    range.start().as_usize()..range.end().as_usize()
}
