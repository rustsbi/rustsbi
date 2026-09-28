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
mod plmt;
mod reset;
pub(crate) mod timer;

use alloc::boxed::Box;

use runtime::memory::MemoryRegistry;

use crate::platform::{BoardInfo, ClintResource, ImsicInfo};

pub(crate) use reset::{
    Description as ResetDescription, ResetDevice, ResetError, ResetReason, ResetRequest, ResetType,
};

pub(crate) use aia::ImsicInterrupt;
pub(crate) use aia::{IMSIC_COMPATIBLES, IMSIC_FILE_SPAN, initialize_hart_imsic};
pub(crate) use cci::Cci550;
pub(crate) use clint::ClintKind;
pub(crate) use console::{ConsoleKind, DbcnBackend, DbcnError};
pub(crate) use ipi::{IpiBackend, IpiError, IpiRequest};
use timer::SstcTimer;
pub(crate) use timer::TimerBackend;

pub(crate) use runtime::hart::HartWake;

pub(crate) const PLMT_COMPATIBLE: &str = "andestech,plmt0";
pub(crate) const SUNXI_PLICSW_COMPATIBLE: &str = "allwinner,sun300i-plicsw";

pub(crate) const THEAD_PLIC_COMPATIBLES: [&str; 2] =
    ["thead,c900-plic", "allwinner,thead,c900-plic"];

/// Platform devices constructed from the discovered hardware description.
pub(crate) struct Devices {
    pub(crate) timer: Option<Box<dyn TimerBackend>>,
    pub(crate) ipi: Option<Box<dyn IpiBackend + Send + Sync>>,
    pub(crate) console: Option<Box<dyn DbcnBackend + Send>>,
    pub(crate) reset: ResetDevice,
}

impl Devices {
    /// Returns whether firmware IPIs use IMSIC interrupt files.
    pub(crate) fn uses_imsic(&self) -> bool {
        self.ipi.as_ref().is_some_and(|ipi| ipi.is_imsic())
    }
}

type InterruptDevices = (
    Option<Box<dyn TimerBackend>>,
    Option<Box<dyn IpiBackend + Send + Sync>>,
);

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
        let (timer, ipi) = aia::bind(imsic, aplic_config, memory)?;
        return Ok((Some(timer), Some(ipi)));
    }
    if let (Some(plmt), Some(plicsw)) = (
        board.devices.interrupts.plmt,
        board.devices.interrupts.plicsw,
    ) {
        let hart_count = board
            .harts
            .enabled
            .iter()
            .rposition(|enabled| *enabled)
            .map(|last| last + 1)
            .ok_or(runtime::Error::InvalidArgs)?;
        return Ok((
            Some(Box::new(plmt::bind(plmt, memory, hart_count)?)),
            Some(Box::new(ipi::plicsw::bind(plicsw, memory, hart_count)?)),
        ));
    }
    let Some(description) = board.devices.interrupts.clint() else {
        return Ok((None, None));
    };
    let ClintResource { registers, kind } = *description.resource();
    let (timer, ipi) = clint::bind(registers, kind, memory)?;
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
    let console = console::bind(board, memory)?;
    let reset = board
        .devices
        .reset
        .bind(board.harts.timebase_frequency_hz, memory)?;
    Ok(Devices {
        timer,
        ipi,
        console,
        reset,
    })
}
