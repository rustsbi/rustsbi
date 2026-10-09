#![forbid(unsafe_code)]

//! One-time platform discovery, resource acquisition, and publication.

use alloc::boxed::Box;
use core::ops::Range;

use runtime::memory::SupervisorMemory;

use super::error::{self, ResultContext};
use super::info::{BoardInfo, SocDescription};
use super::{devices::Devices, discovery, report, state};
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
    let mut board = Box::new(BoardInfo::empty());
    let pmu = platform_description
        .inspect(|platform| discovery::discover_platform(&mut board, &platform))
        .during("reading the platform description")?;

    initialize_platform(platform_description, board, pmu)
}

fn initialize_platform(
    platform_description: runtime::PlatformDescription,
    mut board: Box<BoardInfo>,
    pmu: Option<SbiPmu>,
) -> error::Result<usize> {
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
        Some(SocDescription::V821(description)) => {
            Some(description.prepare(runtime::hart::HartId::count()))
        }
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

    if let Some(v821) = v821.as_ref() {
        driver::allwinner::v821::release_boot0_isp_sram(v821.soc(), &mut memory)
            .during("releasing V821 boot0 ISP SRAM")?;
    }

    if board.devices.interrupts.plmt.is_some()
        && board.devices.interrupts.plicsw.is_some()
        && let Some(v821) = v821.as_ref()
    {
        driver::allwinner::v821::enable_plmt_clock(v821.soc(), &mut memory)
            .during("enabling the V821 PLMT clock")?;
    }

    let mut devices = Devices::bind(&board, &mut memory)?;
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
        let hidden_node_paths = devices
            .interrupts
            .hidden_node_paths(&board)
            .collect::<alloc::vec::Vec<_>>();
        super::handoff::prepare_device_tree(&memory, &hidden_node_paths, platform_description)
            .during("preparing the next-stage platform description")?
            .as_usize()
    };

    devices.hart_wake = k1_resources
        .map(|resources| {
            Box::new(spacemit_k1::initialize_boot_hart(resources)) as Box<dyn HartWake>
        })
        .or_else(|| v861_wake.map(|wake| Box::new(wake) as Box<dyn HartWake>));

    publish_platform_services(&board, supervisor_memory, devices, custom_extension, pmu)?;
    Ok(next_stage_fdt_address)
}

fn publish_platform_services(
    board: &BoardInfo,
    supervisor_memory: SupervisorMemory,
    devices: Devices,
    custom_extension: sbi::vendor::Extension,
    pmu: Option<SbiPmu>,
) -> error::Result<()> {
    state::publish_resources(board, supervisor_memory, devices)
        .during("publishing platform resources")?;
    let devices = state::devices();
    runtime::hart::install_wakeup(devices.hart_wake.as_deref())
        .during("publishing hart-wakeup device")?;
    runtime::timer::Timer::install(devices.interrupts.timer()).during("publishing timer device")?;
    runtime::events::install(pmu.as_ref().map(|_| sbi::pmu::runtime_counters()));

    sbi::logger::Logger::init().expect("BUG: firmware logger initialized more than once");
    info!("Hello RustSBI!");

    let dispatcher = publish_sbi_dispatcher(custom_extension, pmu);
    if let Some(device) = devices.interrupts.ipi() {
        runtime::ipi::Ipi::install(
            device,
            dispatcher
                .ipi
                .as_ref()
                .expect("BUG: bound source has no IPI handler"),
        )
        .during("publishing firmware IPI source and handler")?;
    }

    state::mark_ready();

    report::log_platform_summary(board);
    Ok(())
}

fn publish_sbi_dispatcher(
    custom_extension: sbi::vendor::Extension,
    pmu: Option<SbiPmu>,
) -> &'static SbiDispatcher {
    let devices = state::devices();
    let reset = SbiReset::new(devices.reset.as_ref());
    let supervisor_memory = state::supervisor_memory();
    let console = state::console_device()
        .map(|device| sbi::console::SbiConsole::new(device, supervisor_memory));
    let cppc = Some(SbiCppc::new());
    let dbtr = Some(SbiDbtr::new(supervisor_memory));
    let fwft = Some(SbiFwft);
    let ipi = devices
        .interrupts
        .ipi()
        .map(runtime::ipi::IpiSender::new)
        .map(sbi::ipi::SbiIpi::new);
    let timer = match runtime::timer::Timer::current().and_then(|timer| timer.require_available()) {
        Ok(()) => Some(sbi::timer::SbiTimer),
        Err(runtime::timer::Error::Unavailable) => None,
        Err(error) => panic!("BUG: could not publish SBI TIME: {error}"),
    };
    let hsm = ipi
        .as_ref()
        .map(|_| SbiHsm::new(devices.hart_wake.is_some()));
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
    })
}

/// Runs the SoC-specific per-hart setup for secondary harts.
pub fn initialize_secondary_hart() {
    if let Some(platform) = state::platform().secondary_hart {
        spacemit_k1::initialize_hart(platform);
    }
}

/// Returns the RAM bank containing the linked firmware image.
pub fn firmware_ram_range() -> Range<usize> {
    let range = state::platform().firmware_ram_range;
    range.start().as_usize()..range.end().as_usize()
}
