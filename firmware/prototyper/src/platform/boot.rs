#![forbid(unsafe_code)]

//! One-time platform discovery, resource acquisition, and publication.

use alloc::boxed::Box;
use core::ops::Range;

use runtime::memory::SupervisorMemory;
use spin::Once;

use super::error::{self, ResultContext};
use super::info::{BoardInfo, ImsicInfo, SocDescription};
use super::{discovery, report, state};
use crate::driver::ipi::IpiDevice;
use crate::driver::timer::TimerDevice;
use crate::driver::{self, HartWake};
use crate::riscv::spacemit_k1::{self, K1BootResources};
use crate::sbi;
use crate::sbi::SbiDispatcher;
use crate::sbi::cppc::SbiCppc;
use crate::sbi::dbtr::SbiDbtr;
use crate::sbi::fwft::SbiFwft;
use crate::sbi::hsm::SbiHsm;
use crate::sbi::pmu::SbiPmu;
use crate::sbi::reset::SbiReset;
use crate::sbi::rfence::SbiRFence;
use crate::sbi::suspend::SbiSuspend;

/// Discovers the platform, initializes its devices, and publishes its
/// services. Returns the device tree prepared for the next stage.
pub fn init_board(platform_description: runtime::PlatformDescription) -> usize {
    try_init_board(platform_description).unwrap_or_else(|error| panic!("{error}"))
}

fn try_init_board(platform_description: runtime::PlatformDescription) -> error::Result<usize> {
    let (mut board, pmu) = platform_description
        .inspect(discover_board_and_pmu)
        .during("reading the platform description")?;

    let (supervisor_memory, mut memory) = platform_description
        .memory_resources()
        .during("deriving Runtime memory resources")?;
    board.memory.ram_ranges = memory.ram_ranges().collect();
    let firmware_image_range = memory.firmware_image_range();
    board.memory.firmware_ram_range = Some(
        board
            .memory
            .ram_range_containing(firmware_image_range)
            .ok_or(runtime::Error::InvalidArgs)
            .during("locating the firmware RAM bank")?,
    );

    let v821 = match board.soc.take() {
        Some(SocDescription::V821(description)) => Some(description.prepare(board.harts.count)),
        soc => {
            board.soc = soc;
            None
        }
    }
    .transpose()
    .during("preparing V821 platform resources")?;
    board.memory.noncacheable_alias_offset = v821
        .as_ref()
        .map(crate::platform::allwinner::v821::V821::noncacheable_alias_offset);

    if board.devices.interrupts.plmt.is_some()
        && board.devices.interrupts.plicsw.is_some()
        && let Some(v821) = v821.as_ref()
    {
        driver::allwinner::v821::enable_plmt_clock(v821.soc(), &mut memory)
            .during("enabling the V821 PLMT clock")?;
    }

    let devices = driver::bind_devices(&board, select_imsic(&board), &mut memory)
        .during("binding platform devices")?;
    let custom_extension = sbi::vendor::Extension::bind(v821, &mut memory)
        .during("binding Allwinner custom-extension devices")?;
    let k1_registers = match &board.soc {
        Some(SocDescription::SpacemitK1(registers)) => Some(*registers),
        _ => None,
    };
    let k1_resources = k1_registers
        .map(|registers| K1BootResources::acquire(&mut memory, registers))
        .transpose()
        .during("acquiring SpacemiT K1 resources")?;
    let v861_wake = match &board.soc {
        Some(SocDescription::V861(soc)) => Some(driver::allwinner::v861::V861HartRelease::bind(
            *soc,
            &mut memory,
        )),
        _ => None,
    }
    .transpose()
    .during("initializing V861 C907 resources")?;

    let next_stage_fdt_address = {
        let hidden_node_paths = if devices.uses_imsic() {
            board
                .devices
                .interrupts
                .aia_handoff_paths()
                .collect::<alloc::vec::Vec<_>>()
        } else {
            alloc::vec::Vec::new()
        };
        super::handoff::prepare_device_tree(&memory, &hidden_node_paths, platform_description)
            .during("preparing the next-stage platform description")?
            .as_usize()
    };

    let hart_wake = k1_resources
        .map(|resources| {
            Box::new(spacemit_k1::initialize_boot_hart(resources)) as Box<dyn HartWake>
        })
        .or_else(|| v861_wake.map(|wake| Box::new(wake) as Box<dyn HartWake>));

    publish_platform_services(
        board,
        supervisor_memory,
        devices,
        custom_extension,
        pmu,
        hart_wake,
    );
    Ok(next_stage_fdt_address)
}

/// Selects IMSIC only when every enabled hart can use its CSR interface and
/// Sstc timer. Device construction performs no SBI feature-policy queries.
fn select_imsic(board: &BoardInfo) -> Option<&ImsicInfo> {
    use sbi::features::{self, Extension};

    let imsic = board.devices.interrupts.imsic()?.resource();
    for (hart, enabled) in board.harts.enabled.iter().copied().enumerate() {
        if enabled
            && (!features::hart_has_extension(hart, Extension::Smaia)
                || !features::hart_has_extension(hart, Extension::Sstc))
        {
            warn!(
                "AIA: hart {} requires Smaia and Sstc; falling back to CLINT",
                hart
            );
            return None;
        }
    }
    Some(imsic)
}

fn discover_board_and_pmu(
    platform: runtime::PlatformView<'_>,
) -> runtime::Result<(BoardInfo, Option<SbiPmu>)> {
    let board = discovery::discover_platform(&platform)?;
    let has_v861 = matches!(&board.soc, Some(SocDescription::V861(_)));
    let pmu = sbi::pmu::init(platform.root()).or_else(|| has_v861.then_some(SbiPmu::default()));
    Ok((board, pmu))
}

fn publish_platform_services(
    board: BoardInfo,
    supervisor_memory: SupervisorMemory,
    devices: driver::Devices,
    custom_extension: sbi::vendor::Extension,
    pmu: Option<SbiPmu>,
    hart_wake: Option<Box<dyn HartWake>>,
) {
    let driver::Devices {
        timer,
        ipi,
        console,
        reset,
    } = devices;
    // Hardware ownership is established independently of the SBI dispatcher.
    let external = if ipi.as_ref().is_some_and(|device| device.is_imsic()) {
        static IMSIC: Once<driver::ImsicInterrupt> = Once::new();
        let iid = board
            .devices
            .interrupts
            .imsic()
            .expect("selected IMSIC has a description")
            .resource()
            .ipi_iid;
        Some(IMSIC.call_once(|| driver::ImsicInterrupt::new(iid))
            as &dyn runtime::irq::ExternalInterrupt)
    } else {
        None
    };
    static HART_WAKE: Once<Box<dyn HartWake>> = Once::new();
    let hart_wake = hart_wake.map(|device| HART_WAKE.call_once(|| device).as_ref());
    runtime::hart::install_wakeup(hart_wake);
    let ipi = ipi.map(driver::ipi::init);
    let timer = timer.map(driver::timer::init);
    runtime::ipi::install(ipi.map(|device| device as &dyn runtime::ipi::IpiDevice));
    runtime::ipi::install_handler(ipi.map(|_| sbi::ipi::runtime_handler()));
    runtime::irq::install(external);
    runtime::timer::install(timer.map(|device| device as &dyn runtime::timer::TimerDevice));
    runtime::events::install(pmu.as_ref().map(|_| sbi::pmu::runtime_counters()));

    state::publish_resources(board, supervisor_memory, console);

    sbi::logger::Logger::init().expect("BUG: firmware logger initialized more than once");
    info!("Hello RustSBI!");

    let reset = SbiReset::new(reset);
    publish_sbi_dispatcher(ipi, timer, reset, custom_extension, pmu, hart_wake);

    state::mark_ready();

    report::log_platform_summary();
}

fn publish_sbi_dispatcher(
    ipi: Option<&'static IpiDevice>,
    timer: Option<&'static TimerDevice>,
    reset: SbiReset,
    custom_extension: sbi::vendor::Extension,
    pmu: Option<SbiPmu>,
    hart_wake: Option<&'static dyn HartWake>,
) {
    let supervisor_memory = state::supervisor_memory();
    let console = state::console_device()
        .map(|device| sbi::console::SbiConsole::new(device, supervisor_memory));
    let cppc = Some(SbiCppc::new());
    let dbtr = Some(SbiDbtr::new(supervisor_memory));
    let fwft = Some(SbiFwft);
    let ipi = ipi.map(sbi::ipi::SbiIpi::new);
    let timer = timer.map(sbi::timer::SbiTimer::new);
    let hsm = ipi.as_ref().map(|_| SbiHsm::new(hart_wake.is_some()));
    let rfence = ipi.as_ref().map(|_| SbiRFence);
    let susp = hsm.as_ref().map(|_| SbiSuspend);
    let mpxy = Some(sbi::mpxy::SbiMpxy::new(supervisor_memory));
    let sta = Some(sbi::sta::SbiSta::new(supervisor_memory));
    let vendor = sbi::vendor::Vendor::new(custom_extension, supervisor_memory);

    sbi::SBI_DISPATCHER.call_once(|| SbiDispatcher {
        console,
        cppc,
        dbtr,
        fwft,
        ipi,
        timer,
        hsm,
        reset,
        rfence,
        susp,
        pmu,
        sta,
        mpxy,
        vendor,
    });
}

/// Runs the SoC-specific per-hart setup for secondary harts.
pub fn initialize_secondary_hart() {
    if let Some(SocDescription::SpacemitK1(platform)) = &state::board_info().soc {
        spacemit_k1::initialize_hart(*platform);
    }
}

/// Spins until the boot hart has finished platform initialization.
pub fn wait_until_ready() {
    state::wait_until_ready()
}

/// Returns the RAM bank containing the linked firmware image.
pub fn firmware_ram_range() -> Range<usize> {
    let range = state::board_info()
        .memory
        .firmware_ram_range
        .expect("BUG: firmware RAM bank missing after platform initialization");
    range.start().as_usize()..range.end().as_usize()
}
