#![forbid(unsafe_code)]

//! One-time platform discovery, resource acquisition, and publication.

use alloc::{
    boxed::Box,
    string::{String, ToString},
    vec::Vec,
};
use core::sync::atomic::{AtomicBool, Ordering};
use runtime::{Error, PlatformDescription, Result, console, hart, reset};

use super::{allwinner::v821, describe_pmu, harts, interrupts, report, wakeup};
use crate::{
    driver::{
        Description as _,
        device::{console as console_driver, reset as reset_driver},
    },
    firmware, sbi,
};

static READY: AtomicBool = AtomicBool::new(false);

/// Discovers the platform, initializes its devices, and publishes its services.
///
/// Returns the device tree prepared for the next stage.
pub fn init_platform(platform_description: PlatformDescription) -> Result<usize> {
    let timebase_frequency_hz =
        platform_description.inspect(|platform| harts::describe(&platform))?;
    let model = platform_description.inspect(|platform| {
        Ok(platform
            .root()
            .property("model")
            .and_then(|property| property.as_str())
            .unwrap_or("<unspecified>")
            .to_string())
    })?;

    let console_description =
        platform_description.inspect(|platform| console_driver::select(&platform))?;
    let v821_description = platform_description.inspect(|platform| v821::describe(&platform))?;
    let reset_description = platform_description
        .inspect(|platform| reset_driver::describe(&platform, timebase_frequency_hz))?;
    let pmu = platform_description.inspect(|platform| describe_pmu(&platform))?;

    let (supervisor_memory, mut memory) = platform_description.memory_resources()?;
    let firmware_image_range = memory.firmware_image_range();
    let firmware_ram_range = memory
        .ram_ranges()
        .find(|ram| {
            ram.start() <= firmware_image_range.start() && firmware_image_range.end() <= ram.end()
        })
        .ok_or(Error::InvalidArgs)?;

    let v821 = v821_description
        .map(|description| Box::new(description).init(&mut memory))
        .transpose()?;
    let noncacheable_alias_offset = v821.as_ref().map(v821::V821::noncacheable_alias_offset);

    if let Some(description) = console_description {
        console::init(description.init(&mut memory)?)?;
    }
    sbi::logger::Logger::init().map_err(|_| Error::AlreadyInitialized)?;
    info!("Hello RustSBI!");

    let mut hidden_node_paths: Vec<String> = Vec::new();
    let irq_mmio = platform_description.inspect(|platform| {
        interrupts::init(
            &platform,
            &model,
            v821.as_ref(),
            &mut memory,
            &mut hidden_node_paths,
        )
    })?;
    if let Some(description) = reset_description {
        reset::init(description.init(&mut memory)?)?;
    }
    let custom_extension = platform_description
        .inspect(|platform| sbi::vendor::Extension::init(v821, &platform, &mut memory))?;
    let hart_wake =
        platform_description.inspect(|platform| wakeup::init(&platform, &mut memory))?;

    let next_stage_fdt_address = {
        let hidden_node_paths: Vec<&str> = hidden_node_paths.iter().map(String::as_str).collect();
        super::handoff::prepare_device_tree(&memory, &hidden_node_paths, platform_description)?
            .as_usize()
    };

    // Entry-address validation shares the registry-normalized RAM ranges with
    // PMP installation; this runs before any SBI service is published.
    firmware::init_pmp(
        firmware_ram_range,
        memory.ram_ranges().collect(),
        noncacheable_alias_offset,
        irq_mmio,
    );
    let hardware_wakeup = hart_wake.is_some();
    hart::install_wakeup(hart_wake);
    sbi::init(supervisor_memory, custom_extension, pmu, hardware_wakeup)?;
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
