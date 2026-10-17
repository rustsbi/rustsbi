//! Device drivers.
//!
//! [`bind_devices`] publishes the platform-wide device classes.
//! Device-class discovery stays within each subsystem, while vendor-only
//! devices remain in their vendor module.

#![forbid(unsafe_code)]

mod aia;
pub(crate) mod allwinner;
mod cci;
mod clint;
mod console;
mod plicsw;
mod plmt;
mod reset;

use crate::platform::clint_topology;
use alloc::boxed::Box;
use alloc::vec::Vec;
use runtime::{
    Error, Result, hart::HartId, ipi::IpiDevice, memory::MemoryRegistry, timer::TimerDevice,
};

use crate::platform::{BoardInfo, ImsicInfo};

pub(crate) use reset::{
    Description as ResetDescription, ResetDevice, ResetError, ResetReason, ResetRequest, ResetType,
};

pub(crate) use aia::{IMSIC_COMPATIBLES, IMSIC_FILE_SPAN, initialize_hart_imsic};
pub(crate) use cci::Cci550;
pub(crate) use clint::ClintKind;
pub(crate) use console::{ConsoleKind, DbcnBackend, DbcnError};

pub(crate) use runtime::hart::HartWake;

pub(crate) const PLMT_COMPATIBLE: &str = "andestech,plmt0";
pub(crate) const SUNXI_PLICSW_COMPATIBLE: &str = "allwinner,sun300i-plicsw";

pub(crate) const THEAD_PLIC_COMPATIBLES: [&str; 2] =
    ["thead,c900-plic", "allwinner,thead,c900-plic"];

/// Platform devices constructed from the discovered hardware description.
pub(crate) struct Devices {
    pub(crate) timer: Option<Box<dyn TimerDevice>>,
    pub(crate) ipi: Option<Box<dyn IpiDevice>>,
    pub(crate) console: Option<Box<dyn DbcnBackend + Send>>,
    pub(crate) reset: ResetDevice,
}

type InterruptDevices = (Option<Box<dyn TimerDevice>>, Option<Box<dyn IpiDevice>>);

fn bind_interrupts(
    board: &BoardInfo,
    selected_imsic: Option<&ImsicInfo>,
    memory: &mut MemoryRegistry,
) -> Result<InterruptDevices> {
    if let Some(imsic) = selected_imsic {
        // The QEMU AIA PMP layout reserves only one CLINT MMIO interval.
        // Reject unsupported multi-CLINT AIA rather than exposing other machine registers.
        if board.is_qemu_virt() && board.devices.interrupts.clints().len() > 1 {
            return Err(Error::InvalidArgs);
        }
        let aplic_config = if board.is_qemu_virt() {
            Some(crate::platform::qemu_aplic::QemuAplicConfig::new(
                board
                    .devices
                    .interrupts
                    .machine_aplic()
                    .map(|description| *description.resource())
                    .ok_or(Error::InvalidArgs)?,
                imsic.layout.machine_base,
                imsic.layout.hart_index_bits,
            )?)
        } else {
            warn!("AIA: skipping QEMU virt M-APLIC setup on '{}'", board.model);
            None
        };
        let ipi = aia::bind(imsic, aplic_config, memory)?;
        return Ok((None, Some(ipi)));
    }
    if let (Some(plmt), Some(plicsw)) = (
        board.devices.interrupts.plmt,
        board.devices.interrupts.plicsw,
    ) {
        let hart_id_upper_bound = HartId::all()
            .last()
            .and_then(|hart| hart.as_usize().checked_add(1))
            .ok_or(Error::InvalidArgs)?;
        return Ok((
            Some(Box::new(plmt::bind(plmt, memory, hart_id_upper_bound)?)),
            Some(Box::new(plicsw::bind(plicsw, memory, hart_id_upper_bound)?)),
        ));
    }
    let descriptions = board.devices.interrupts.clints();
    if descriptions.is_empty() {
        return Ok((None, None));
    }
    // Reject intersecting hardware windows before any MMIO resources are acquired.
    if !clint_topology::disjoint_ranges(descriptions.iter().map(|d| {
        let reg = d.resource().registers;
        (reg.start().as_usize(), reg.end().as_usize())
    })) {
        return Err(Error::InvalidArgs);
    }
    let routes = clint_topology::build_routes(
        HartId::count(),
        descriptions.iter().map(|d| {
            let res = d.resource();
            (res.active_harts.as_slice(), res.slot_count)
        }),
        |raw| HartId::from_raw(raw).ok().map(|hart| hart.index()),
    )
    .map_err(|_| Error::InvalidArgs)?;
    let mut timers = Vec::new();
    let mut ipis = Vec::new();
    for description in descriptions {
        let resource = description.resource();
        let (timer, ipi) = clint::bind(
            resource.registers,
            resource.kind,
            memory,
            resource.slot_count,
        )?;
        timers.push(timer);
        ipis.push(ipi);
    }
    Ok((
        Some(Box::new(clint::RoutedTimer::new(timers, routes.clone()))),
        Some(Box::new(clint::RoutedIpi::new(ipis, routes))),
    ))
}

/// Binds all devices selected during platform discovery.
pub(crate) fn bind_devices(
    board: &BoardInfo,
    selected_imsic: Option<&ImsicInfo>,
    memory: &mut MemoryRegistry,
) -> Result<Devices> {
    if let Some(registers) = board.devices.interrupts.thead_plic {
        // T-Head PLIC_CTRL bit 0 delegates access to S-mode.
        let control = memory.acquire(registers.subrange(0x1ffffc, size_of::<u32>())?)?;
        control.write(0, 1u32)?;
    }
    let (timer, ipi) = bind_interrupts(board, selected_imsic, memory)?;
    let console = console::bind(board, memory)?;
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
