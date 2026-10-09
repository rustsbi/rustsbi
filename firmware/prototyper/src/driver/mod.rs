//! Device drivers.
//!
//! [`bind_devices`] publishes the platform-wide device classes. Device-class
//! discovery stays within each subsystem, while vendor-only devices remain in
//! their vendor module.

#![forbid(unsafe_code)]

mod aia;
pub(crate) mod allwinner;
mod cci;
mod clint;
mod console;
pub(crate) mod ipi;
mod plicsw;
mod plmt;
mod reset;
pub(crate) mod timer;

use alloc::boxed::Box;

use runtime::memory::MemoryRegistry;

use crate::platform::{BoardInfo, ClintResource, ImsicInfo};

pub(crate) use reset::{
    Description as ResetDescription, ResetController, ResetError, ResetReason, ResetRequest,
    ResetType,
};

pub(crate) use aia::{IMSIC_COMPATIBLES, IMSIC_FILE_SPAN};
pub(crate) use cci::Cci550;
pub(crate) use clint::ClintKind;
pub(crate) use console::{Console, ConsoleError, ConsoleKind};
use runtime::ipi::{InterruptSource, IpiDevice};
use runtime::timer::TimerDevice;

pub(crate) use runtime::hart::HartWake;

pub(crate) const PLMT_COMPATIBLE: &str = "andestech,plmt0";
pub(crate) const SUNXI_PLICSW_COMPATIBLE: &str = "allwinner,sun300i-plicsw";

pub(crate) const THEAD_PLIC_COMPATIBLES: [&str; 2] =
    ["thead,c900-plic", "allwinner,thead,c900-plic"];

/// Platform devices constructed from the discovered hardware description.
pub(crate) struct Devices {
    pub(crate) timer: Option<Box<dyn TimerDevice>>,
    pub(crate) ipi: Option<Box<dyn IpiDevice>>,
    pub(crate) console: Option<Console>,
    pub(crate) reset: Option<ResetController>,
}

impl Devices {
    /// Returns whether firmware IPIs use IMSIC interrupt files.
    pub(crate) fn uses_imsic(&self) -> bool {
        self.ipi
            .as_ref()
            .is_some_and(|ipi| matches!(ipi.interrupt_source(), InterruptSource::Imsic(_)))
    }
}

type InterruptDevices = (Option<Box<dyn TimerDevice>>, Option<Box<dyn IpiDevice>>);

fn bind_interrupts(
    board: &BoardInfo,
    selected_imsic: Option<&ImsicInfo>,
    memory: &mut MemoryRegistry,
) -> runtime::Result<InterruptDevices> {
    if let Some(imsic) = selected_imsic {
        let aplic_config = if board.is_qemu_virt() {
            Some(crate::platform::qemu_aplic::QemuAplicConfig::new(
                board
                    .devices
                    .interrupts
                    .machine_aplic()
                    .map(|description| *description.resource())
                    .ok_or(runtime::Error::InvalidArgs)?,
                imsic.layout.machine_base,
                imsic.layout.hart_index_bits,
            )?)
        } else {
            warn!("AIA: skipping QEMU virt M-APLIC setup on '{}'", board.model);
            None
        };
        let ipi = aia::bind(imsic, memory)?;
        if let Some(aplic_config) = aplic_config {
            aplic_config.bind(memory)?;
        }
        return Ok((None, Some(ipi)));
    }
    if let (Some(plmt), Some(plicsw)) = (
        board.devices.interrupts.plmt,
        board.devices.interrupts.plicsw,
    ) {
        let hart_id_upper_bound = runtime::hart::HartId::all()
            .last()
            .and_then(|hart| hart.as_usize().checked_add(1))
            .ok_or(runtime::Error::InvalidArgs)?;
        return Ok((
            Some(Box::new(plmt::bind(plmt, memory, hart_id_upper_bound)?)),
            Some(plicsw::bind(plicsw, memory, hart_id_upper_bound)?),
        ));
    }
    let Some(description) = board.devices.interrupts.clint() else {
        return Ok((None, None));
    };
    let ClintResource { registers, kind } = *description.resource();
    let hart_id_upper_bound = runtime::hart::HartId::all()
        .last()
        .and_then(|hart| hart.as_usize().checked_add(1))
        .ok_or(runtime::Error::InvalidArgs)?;
    let (timer, ipi) = clint::bind(registers, kind, memory, hart_id_upper_bound)?;
    Ok((Some(timer), Some(ipi)))
}

/// Binds all devices selected during platform discovery.
pub(crate) fn bind_devices(
    board: &BoardInfo,
    selected_imsic: Option<&ImsicInfo>,
    memory: &mut MemoryRegistry,
) -> runtime::Result<Devices> {
    if let Some(registers) = board.devices.interrupts.thead_plic {
        // T-Head PLIC_CTRL bit 0 delegates access to S-mode.
        let control = memory.acquire_mmio(registers.subrange(0x1ffffc, size_of::<u32>())?)?;
        control.write(0, 1u32)?;
    }
    let (timer, ipi) = bind_interrupts(board, selected_imsic, memory)?;
    let console = board
        .devices
        .console
        .as_ref()
        .map(|console| Console::bind(console.registers, console.kind, console.clock_hz, memory))
        .transpose()?;
    let reset = board
        .devices
        .reset
        .bind(board.timebase_frequency_hz, memory)?;
    Ok(Devices {
        timer,
        ipi,
        console,
        reset,
    })
}
