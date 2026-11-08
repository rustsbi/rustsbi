//! Selection and initialization of the platform's timer and IPI devices.

use alloc::{boxed::Box, string::String, vec::Vec};
use runtime::{
    Error, PlatformView, Result,
    ipi::{self, Ipi},
    memory::MemoryRegistry,
    timer,
};

use super::{allwinner::v821, qemu_aplic};
use crate::{
    devicetree::NodeSelection,
    driver::{
        Description as _,
        device::{clint, interrupts, interrupts::Selection, plicsw, thead_plic},
    },
    firmware::IrqMmio,
};

/// Parses, selects, and registers the timer and IPI devices for this platform.
///
/// When IMSIC is selected, appends the device-tree paths to hide from the next stage.
/// Returns QEMU interrupt MMIO ranges when required for PMP.
pub(super) fn init(
    platform: &PlatformView<'_>,
    model: &str,
    v821: Option<&v821::V821>,
    memory: &mut MemoryRegistry,
    hidden_node_paths: &mut Vec<String>,
) -> Result<Option<IrqMmio>> {
    let mut selected = interrupts::describe(platform)?;

    if selected.plmt.is_some() && selected.plicsw.is_some() {
        v821::prepare_plmt(v821, memory)?;
    }
    if let Some(registers) = selected.thead_plic {
        thead_plic::init(registers, memory)?;
    }

    let clint = init_timer(&mut selected, memory)?;
    init_ipi(&mut selected, clint, model, memory, hidden_node_paths)
}

/// Initializes the preferred PLMT timer pair or falls back to the CLINT timer.
///
/// Returns the CLINT IPI description and its source path for IPI selection.
fn init_timer(
    selected: &mut Selection,
    memory: &mut MemoryRegistry,
) -> Result<Option<NodeSelection<clint::ipi::Desc>>> {
    let (clint, clint_timer) = match selected.clint.take() {
        Some(node) => {
            let (clint, path) = node.into_parts();
            let clint::Desc { ipi, timer } = clint;
            (Some(NodeSelection::new(ipi, path)), Some(timer))
        }
        None => (None, None),
    };

    // PLMT and PLICSW are initialized only as a pair; otherwise the CLINT
    // timer is the fallback.
    if let (Some(plmt), Some(_)) = (selected.plmt.take(), selected.plicsw.as_ref()) {
        timer::init(Some(Box::new(plmt).init(memory)?))?;
    } else {
        selected.plicsw = None;
        timer::init(clint_timer.map(|timer| timer.init(memory)).transpose()?)?;
    }
    Ok(clint)
}

/// Registers IMSIC when available, or falls back to a software IPI device.
fn init_ipi(
    selected: &mut Selection,
    clint: Option<NodeSelection<clint::ipi::Desc>>,
    model: &str,
    memory: &mut MemoryRegistry,
    hidden_node_paths: &mut Vec<String>,
) -> Result<Option<IrqMmio>> {
    if let Some(node) = selected.imsic.take() {
        let (imsic, imsic_path) = node.into_parts();
        let machine_base = imsic.layout.machine_base;
        let hart_index_bits = imsic.layout.hart_index_bits;
        let imsic_end = imsic
            .hart_files
            .iter()
            .map(|range| range.end().as_usize())
            .max();

        match Box::new(imsic).init(memory) {
            Ok(device) => {
                let irq_mmio = qemu_aplic::init_for_imsic(
                    model,
                    selected.machine_aplic.as_ref().map(|node| *node.resource()),
                    clint.as_ref().map(|clint| clint.resource().registers),
                    machine_base,
                    hart_index_bits,
                    imsic_end,
                    memory,
                )?;
                ipi::init(Ipi::external(device)?)?;
                if let Some(clint) = clint {
                    hidden_node_paths.push(clint.into_parts().1);
                }
                hidden_node_paths.push(imsic_path);
                if selected.machine_aplic_delegated
                    && let Some(aplic) = selected.machine_aplic.take()
                {
                    hidden_node_paths.push(aplic.into_parts().1);
                }
                return Ok(irq_mmio);
            }
            Err(Error::NotSupported) => {}
            Err(error) => return Err(error),
        }
    }

    init_software_ipi(selected.plicsw.take(), clint.as_ref(), memory)?;
    Ok(None)
}

/// Registers a software IPI device from PLICSW or CLINT, if available.
fn init_software_ipi(
    plicsw: Option<plicsw::Desc>,
    clint: Option<&NodeSelection<clint::ipi::Desc>>,
    memory: &mut MemoryRegistry,
) -> Result<()> {
    match (plicsw, clint) {
        (Some(plicsw), _) => ipi::init(Ipi::software(Box::new(plicsw).init(memory)?)?),
        (None, Some(clint)) => ipi::init(Ipi::software(Box::new(*clint.resource()).init(memory)?)?),
        (None, None) => Ok(()),
    }
}
