//! Selection and initialization of the platform's timer, IPI, and irqchip devices.

use alloc::{boxed::Box, string::String, vec::Vec};
use runtime::{
    Error, PlatformView, Result,
    ipi::{self, Ipi},
    memory::MemoryRegistry,
    timer,
};

use super::{allwinner::v821, qemu_aplic};
use crate::{
    driver::{
        Description as _,
        device::{
            interrupts, interrupts::IpiSelection, interrupts::IrqchipSelection,
            interrupts::TimerSelection, mswi, plicsw, thead_plic,
        },
    },
    firmware::IrqMmio,
};

/// Parses, selects, and registers the timer and IPI devices for this platform.
///
/// When IMSIC is selected, appends the device-tree paths to hide from the next stage.
/// Returns QEMU interrupt MMIO ranges when required for PMP.
pub(super) fn init(
    platform: &PlatformView<'_>,
    v821: Option<&v821::V821>,
    memory: &mut MemoryRegistry,
    hidden_node_paths: &mut Vec<String>,
) -> Result<Option<IrqMmio>> {
    let mut selected = interrupts::describe(platform)?;

    if selected.timer.plmt.is_some() && selected.ipi.plicsw.is_some() {
        v821::prepare_plmt(v821, memory)?;
    }
    if let Some(registers) = selected.irqchip.thead_plic {
        thead_plic::init(registers, memory)?;
    }

    let aclint_timer_path = init_timer(&mut selected.timer, &mut selected.ipi, memory)?;
    init_ipi(
        &mut selected.ipi,
        &mut selected.irqchip,
        aclint_timer_path.as_deref(),
        memory,
        hidden_node_paths,
    )
}

/// Initializes the preferred PLMT, ACLINT, or CLINT timer.
///
/// Returns the ACLINT MTIMER node path so the IMSIC handoff can hide the
/// machine-level timer from the next stage, mirroring the CLINT hiding.
fn init_timer(
    selected: &mut TimerSelection,
    ipi: &mut IpiSelection,
    memory: &mut MemoryRegistry,
) -> Result<Option<String>> {
    // PLMT and PLICSW are initialized only as a pair.
    if let (Some(plmt), Some(_)) = (selected.plmt.take(), ipi.plicsw.as_ref()) {
        timer::init(Some(Box::new(plmt).init(memory)?))?;
        Ok(None)
    } else if let Some(node) = selected.aclint.take() {
        let (mtimer, path) = node.into_parts();
        timer::init(Some(Box::new(mtimer).init(memory)?))?;
        info!("{:<30}: {}", "Platform Timer Extension", "ACLINT MTIMER");
        Ok(Some(path))
    } else {
        ipi.plicsw = None;
        timer::init(
            selected
                .clint
                .take()
                .map(|timer| timer.into_parts().0)
                .map(|timer| Box::new(timer).init(memory))
                .transpose()?,
        )?;
        Ok(None)
    }
}

/// Registers IMSIC when available, or falls back to a software IPI device.
fn init_ipi(
    selected: &mut IpiSelection,
    irqchip: &mut IrqchipSelection,
    aclint_timer_path: Option<&str>,
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
                let clint_registers = selected
                    .clint
                    .as_ref()
                    .map(|clint| clint.resource().registers);
                let irq_mmio = match irqchip.machine_aplic.take() {
                    Some(node) => {
                        let (desc, aplic_path) = node.into_parts();
                        let irq_mmio = qemu_aplic::init_for_imsic(
                            desc,
                            clint_registers,
                            machine_base,
                            hart_index_bits,
                            imsic_end,
                            memory,
                        )?;
                        if irqchip.machine_aplic_delegated {
                            hidden_node_paths.push(aplic_path);
                        }
                        Some(irq_mmio)
                    }
                    None => {
                        warn!("AIA: no machine-level APLIC described; keeping supervisor wiring");
                        None
                    }
                };
                ipi::init(Ipi::external(device)?)?;
                if let Some(clint) = selected.clint.take() {
                    hidden_node_paths.push(clint.into_parts().1);
                }
                hidden_node_paths.push(imsic_path);
                if let Some(path) = aclint_timer_path {
                    hidden_node_paths.push(String::from(path));
                }
                return Ok(irq_mmio);
            }
            Err(Error::NotSupported) => {}
            Err(error) => return Err(error),
        }
    }

    init_software_ipi(
        selected.plicsw.take(),
        selected.aclint.take().map(|node| node.into_parts().0),
        selected.clint.take().map(|node| node.into_parts().0),
        memory,
    )?;
    Ok(None)
}

/// Registers a software IPI device from PLICSW, ACLINT MSWI, or CLINT.
fn init_software_ipi(
    plicsw: Option<plicsw::Desc>,
    aclint_mswi: Option<mswi::Desc>,
    clint: Option<mswi::Desc>,
    memory: &mut MemoryRegistry,
) -> Result<()> {
    match (plicsw, aclint_mswi, clint) {
        (Some(plicsw), _, _) => ipi::init(Ipi::software(Box::new(plicsw).init(memory)?)?),
        (None, Some(mswi), _) => {
            ipi::init(Ipi::software(Box::new(mswi).init(memory)?)?)?;
            info!("{:<30}: {}", "Platform IPI Extension", "ACLINT MSWI");
            Ok(())
        }
        (None, None, Some(clint)) => ipi::init(Ipi::software(Box::new(clint).init(memory)?)?),
        (None, None, None) => Ok(()),
    }
}
