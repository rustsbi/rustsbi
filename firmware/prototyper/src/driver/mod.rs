//! Device drivers.
//!
//! [`bind_devices`] publishes the platform-wide device classes.
//! Device-class discovery stays within each subsystem, while vendor-only
//! devices remain in their vendor module.

#![forbid(unsafe_code)]

pub(crate) mod clint;
mod console;
mod device;
mod ipi;
pub(crate) mod irqchip;
mod reset;
mod timer;

use alloc::{boxed::Box, vec::Vec};
use runtime::{
    PlatformView, Result,
    ipi::IpiDevice,
    memory::{DeviceRegisterRange, MemoryRegistry},
    timer::TimerDevice,
};

pub(crate) use device::DeviceSummary;
pub(crate) use reset::{ResetDevice, ResetError, ResetReason, ResetRequest, ResetType};

pub(crate) use console::{ConsoleDevice, ConsoleError, ConsoleSummary};
pub(crate) use irqchip::aia::{IMSIC_COMPATIBLES, IMSIC_FILE_SPAN, ImsicInfo};

pub(crate) use runtime::hart::HartWake;

pub(crate) const PLMT_COMPATIBLE: &str = "andestech,plmt0";
pub(crate) const SUNXI_PLICSW_COMPATIBLE: &str = "allwinner,sun300i-plicsw";

pub(crate) const THEAD_PLIC_COMPATIBLES: [&str; 2] =
    ["thead,c900-plic", "allwinner,thead,c900-plic"];

const THEAD_PLIC_CTRL_OFFSET: usize = 0x1f_fffc;
const THEAD_PLIC_SUPERVISOR_ACCESS: u32 = 1 << 0;

/// Discovery facts consumed by device binding.
pub(crate) struct DeviceFacts<'a> {
    pub(crate) thead_plic: Option<DeviceRegisterRange>,
    pub(crate) machine_aplic: Option<DeviceRegisterRange>,
    pub(crate) imsic: Option<&'a ImsicInfo>,
}

/// Platform devices constructed from the discovered hardware description.
pub(crate) struct Devices {
    pub(crate) timer: Option<Box<dyn TimerDevice>>,
    pub(crate) ipi: Option<Box<dyn IpiDevice>>,
    pub(crate) console: Option<Box<dyn ConsoleDevice>>,
    pub(crate) console_summary: Option<ConsoleSummary>,
    pub(crate) clint_summary: Option<DeviceSummary>,
    pub(crate) reset: Vec<Box<dyn ResetDevice>>,
}

struct InterruptDevices {
    timer: Option<Box<dyn TimerDevice>>,
    ipi: Option<Box<dyn IpiDevice>>,
    timer_summary: Option<DeviceSummary>,
    ipi_summary: Option<DeviceSummary>,
}

fn bind_interrupts(
    platform: &PlatformView<'_>,
    facts: &DeviceFacts<'_>,
    memory: &mut MemoryRegistry,
) -> Result<InterruptDevices> {
    if let Some(imsic) = facts.imsic {
        let aplic_config = irqchip::aplic::config(platform, facts, imsic)?;
        let ipi = irqchip::aia::bind(imsic, aplic_config, memory)?;
        return Ok(InterruptDevices {
            timer: None,
            ipi: Some(ipi),
            timer_summary: None,
            ipi_summary: None,
        });
    }
    let timer_description = timer::DRIVERS.select(platform, None)?;
    let timer_summary = timer_description.as_ref().map(device::Description::summary);
    let timer = match timer_description {
        Some(description) => Some(description.bind(memory)?),
        None => None,
    };
    let ipi_description = ipi::DRIVERS.select(platform, None)?;
    let ipi_summary = ipi_description.as_ref().map(device::Description::summary);
    let ipi = match ipi_description {
        Some(description) => Some(description.bind(memory)?),
        None => None,
    };
    Ok(InterruptDevices {
        timer,
        ipi,
        timer_summary,
        ipi_summary,
    })
}

/// Binds all devices selected during platform discovery.
pub(crate) fn bind_devices(
    platform: &PlatformView<'_>,
    facts: &DeviceFacts<'_>,
    memory: &mut MemoryRegistry,
) -> Result<Devices> {
    if let Some(registers) = facts.thead_plic {
        // T-Head PLIC_CTRL bit 0 delegates access to S-mode.
        let control =
            memory.acquire(registers.subrange(THEAD_PLIC_CTRL_OFFSET, size_of::<u32>())?)?;
        control.write(0, THEAD_PLIC_SUPERVISOR_ACCESS)?;
    }
    let InterruptDevices {
        timer,
        ipi,
        timer_summary,
        ipi_summary,
    } = bind_interrupts(platform, facts, memory)?;
    let clint_summary = match (timer_summary, ipi_summary) {
        (Some(timer), Some(ipi)) if timer.registers == ipi.registers => Some(timer),
        _ => None,
    };
    let console_description =
        console::DRIVERS.select(platform, crate::boot::stdout_node(platform))?;
    let console_summary = console_description
        .as_ref()
        .map(|description| ConsoleSummary {
            name: description.name(),
            registers: description.registers(),
        });
    let console = match console_description {
        Some(description) => Some(description.bind(memory)?),
        None => None,
    };
    let reset = reset::DRIVERS
        .bind_first_by_driver(platform, memory)?
        .into_iter()
        .collect();
    Ok(Devices {
        timer,
        ipi,
        console,
        console_summary,
        clint_summary,
        reset,
    })
}
