//! Initialization of the platform's timer and interrupt devices.

use alloc::{boxed::Box, string::String, vec::Vec};
use runtime::{
    Error, Result,
    ipi::{self, Ipi, IpiDevice, Software},
    memory::MemoryRegistry,
    timer::{self, TimerDevice},
};

use super::qemu_aplic;
use crate::{
    driver::device::{
        imsic::ImsicIpi,
        interrupts::{IpiSelection, IrqchipSelection, TimerSelection},
        mswi::Mswi,
        mtimer::{self, Mtimer},
        plicsw::PlicSw,
        plmt::{self, Plmt},
        thead_plic,
    },
    firmware::IrqMmio,
};

/// A selected machine timer before MMIO acquisition.
pub(crate) enum TimerDesc {
    Plmt(plmt::Desc),
    Mtimer(mtimer::Desc),
}

/// Selects a timer and retains only the usable software-IPI candidates.
pub(super) fn select_timer(
    candidates: TimerSelection,
    ipi: &mut IpiSelection,
) -> Option<TimerDesc> {
    // PLMT is usable only with PLICSW. ACLINT fallback can still use PLICSW.
    if let Some(timer) = candidates.plmt.filter(|_| ipi.plicsw.is_some()) {
        return Some(TimerDesc::Plmt(timer));
    }
    if let Some(timer) = candidates.aclint {
        return Some(TimerDesc::Mtimer(timer));
    }
    ipi.plicsw = None;
    candidates.clint.map(TimerDesc::Mtimer)
}

/// Initializes the platform's timer and interrupt devices.
///
/// Returns the interrupt MMIO ranges required by PMP and the device-tree
/// paths to hide when handing off to the next stage.
pub(super) fn init(
    selected_timer: Option<TimerDesc>,
    ipi: IpiSelection,
    irqchip: IrqchipSelection,
    model: &str,
    memory: &mut MemoryRegistry,
) -> Result<(Option<IrqMmio>, Vec<String>)> {
    if let Some(registers) = irqchip.thead_plic {
        thead_plic::init(registers, memory)?;
    }

    let timer_device: Option<Box<dyn TimerDevice>> = match selected_timer {
        Some(TimerDesc::Plmt(description)) => Some(Box::new(Plmt::init(description, memory)?)),
        Some(TimerDesc::Mtimer(description)) => Some(Box::new(Mtimer::init(description, memory)?)),
        None => None,
    };
    timer::init(timer_device)?;
    init_ipi(ipi, irqchip, model, memory)
}

/// Prefers IMSIC for IPIs and falls back to software when IMSIC is unsupported.
fn init_ipi(
    selected: IpiSelection,
    irqchip: IrqchipSelection,
    model: &str,
    memory: &mut MemoryRegistry,
) -> Result<(Option<IrqMmio>, Vec<String>)> {
    if let Some(node) = selected.imsic {
        let (imsic, imsic_path) = node.into_parts();
        let machine_base = imsic.layout.machine_base;
        let hart_index_bits = imsic.layout.hart_index_bits;
        let imsic_end = imsic
            .hart_files
            .iter()
            .map(|range| range.end().as_usize())
            .max();

        match ImsicIpi::init(imsic, memory) {
            Ok(device) => {
                let device = Box::new(device);
                let irq_mmio = qemu_aplic::init_for_imsic(
                    model,
                    irqchip.machine_aplic.as_ref().map(|node| *node.resource()),
                    selected
                        .clint
                        .as_ref()
                        .map(|clint| clint.resource().registers),
                    machine_base,
                    hart_index_bits,
                    imsic_end,
                    memory,
                )?;
                ipi::init(Ipi::external(device)?)?;
                // Collect handoff exclusions only after IMSIC becomes active,
                // so software fallback leaves its interrupt nodes visible.
                let mut hidden_node_paths = Vec::new();
                if let Some(clint) = selected.clint {
                    hidden_node_paths.push(clint.into_parts().1);
                }
                hidden_node_paths.push(imsic_path);
                if irqchip.machine_aplic_delegated
                    && let Some(aplic) = irqchip.machine_aplic
                {
                    hidden_node_paths.push(aplic.into_parts().1);
                }
                return Ok((irq_mmio, hidden_node_paths));
            }
            Err(Error::NotSupported) => {}
            Err(error) => return Err(error),
        }
    }

    let ipi_device: Option<Box<dyn IpiDevice<Software>>> =
        match (selected.plicsw, selected.aclint, selected.clint) {
            (Some(plicsw), _, _) => Some(Box::new(PlicSw::init(plicsw, memory)?)),
            (None, Some(mswi), _) => Some(Box::new(Mswi::init(mswi, memory)?)),
            (None, None, Some(clint)) => {
                let (clint, _) = clint.into_parts();
                Some(Box::new(Mswi::init(clint, memory)?))
            }
            (None, None, None) => None,
        };
    if let Some(device) = ipi_device {
        ipi::init(Ipi::software(device)?)?;
    }
    Ok((None, Vec::new()))
}
