#![forbid(unsafe_code)]

//! One-time platform discovery, resource acquisition, and publication.

use alloc::{
    string::{String, ToString},
    vec::Vec,
};
use core::sync::atomic::{AtomicBool, Ordering};
use runtime::{
    Error, PlatformDescription, PlatformView, Result, console, memory::SupervisorMemory,
};
use spin::Once;

use super::{Board, harts, interrupts, report};
use crate::{
    devicetree::try_for_each_enabled_node,
    driver::device::{
        console as console_driver, interrupts as interrupt_drivers, reset as reset_driver,
    },
    firmware,
    sbi::{
        self,
        pmu::{self, SbiPmu},
    },
};

static SUPERVISOR_MEMORY: Once<SupervisorMemory> = Once::new();
static READY: AtomicBool = AtomicBool::new(false);

/// Runs the common startup sequence with the selected board's operations.
pub(crate) fn init(platform_description: PlatformDescription, board: &dyn Board) -> Result<usize> {
    let (model, timebase_frequency_hz, console_description, reset_description, mut pmu, interrupts) =
        platform_description.inspect(|platform| {
            let timebase_frequency_hz = harts::describe(&platform)?;
            let model = platform
                .root()
                .property("model")
                .and_then(|property| property.as_str())
                .unwrap_or("<unspecified>")
                .to_string();
            let console = console_driver::select(&platform)?;
            let reset = reset_driver::describe(&platform, timebase_frequency_hz)?;
            let pmu = describe_pmu(&platform)?;
            let interrupts = interrupt_drivers::describe(&platform)?;
            Ok((
                model,
                timebase_frequency_hz,
                console,
                reset,
                pmu,
                interrupts,
            ))
        })?;

    let (supervisor_memory, mut memory) = platform_description.memory_resources()?;
    let firmware_image_range = memory.firmware_image_range();
    let firmware_ram_range = memory
        .ram_ranges()
        .find(|ram| {
            ram.start() <= firmware_image_range.start() && firmware_image_range.end() <= ram.end()
        })
        .ok_or(Error::InvalidArgs)?;
    let interrupt_drivers::Selection {
        timer,
        mut ipi,
        irqchip,
    } = interrupts;
    let timer = interrupts::select_timer(timer, &mut ipi);
    let noncacheable_alias_offset = board.early_init(&mut memory, timer.as_ref())?;

    if let Some(description) = console_description {
        console::init(description.init(&mut memory)?)?;
    }
    sbi::logger::Logger::init().map_err(|_| Error::AlreadyInitialized)?;
    info!("Hello RustSBI!");

    let (irq_mmio, hidden_node_paths) = interrupts::init(timer, ipi, irqchip, &model, &mut memory)?;
    if let Some(description) = reset_description {
        reset_driver::init(description, &mut memory)?;
    }

    let supervisor_memory = SUPERVISOR_MEMORY.call_once(|| supervisor_memory);
    let vendor = platform_description
        .inspect(|view| board.init(&view, &mut memory, supervisor_memory, &mut pmu))?;

    let hidden_node_paths: Vec<_> = hidden_node_paths.iter().map(String::as_str).collect();
    let firmware_reservation =
        (!memory.firmware_is_reserved()).then_some(memory.firmware_image_range());
    let next_stage_fdt_address = platform_description
        .prepare_next_stage(firmware_reservation, &hidden_node_paths)?
        .as_usize();

    firmware::init_pmp(firmware_ram_range, noncacheable_alias_offset, irq_mmio);
    sbi::init(
        supervisor_memory,
        vendor,
        pmu,
        runtime::hart::wakeup_installed(),
    )?;
    // The release pairs with `wait_until_ready` so secondary harts observe
    // all initialized services before proceeding.
    READY.store(true, Ordering::Release);
    report::log_platform_summary(&model, timebase_frequency_hz, memory.ram_ranges());
    Ok(next_stage_fdt_address)
}

/// Spins until the boot hart has finished platform initialization.
pub fn wait_until_ready() {
    while !READY.load(Ordering::Acquire) {
        core::hint::spin_loop();
    }
}

/// Reads PMU event mappings from the first enabled PMU node.
fn describe_pmu(platform: &PlatformView<'_>) -> Result<Option<SbiPmu>> {
    let mut pmu_node = None;
    try_for_each_enabled_node(platform, |node, _| {
        if pmu_node.is_none()
            && node
                .compatible()
                .is_some_and(|values| values.all().any(|value| value == "riscv,pmu"))
        {
            pmu_node = Some(node.node());
        }
        Ok(())
    })?;
    Ok(pmu_node.and_then(pmu::from_node))
}
