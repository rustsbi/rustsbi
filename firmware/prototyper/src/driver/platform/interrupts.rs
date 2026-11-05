//! Selection and initialization of the platform's timer and IPI devices.

use alloc::{boxed::Box, string::String, vec::Vec};
use runtime::{
    Error, PlatformView, Result,
    ipi::{self, Ipi},
    memory::MemoryRegistry,
    timer,
};

use super::qemu_aplic;
use crate::{
    devicetree::{select_from_node_once, try_for_each_enabled_node},
    driver::{
        Description as _,
        device::{clint, imsic, plicsw, plmt, thead_plic},
        match_desc,
    },
    firmware::IrqMmio,
};

/// Parses, selects, and registers the timer and IPI devices for this platform.
///
/// Calls `prepare_plmt` before initialization when both PLMT and PLICSW are found.
/// When IMSIC is selected, appends the device-tree paths to hide from the next stage.
/// Returns QEMU interrupt MMIO ranges when required for PMP.
pub(super) fn init(
    platform: &PlatformView<'_>,
    model: &str,
    cpu_interrupt_controllers: &[imsic::CpuInterruptController],
    memory: &mut MemoryRegistry,
    hidden_node_paths: &mut Vec<String>,
    prepare_plmt: impl FnOnce(&mut MemoryRegistry) -> Result<()>,
) -> Result<Option<IrqMmio>> {
    let mut clint = None;
    let mut imsic = None;
    let mut machine_aplic = None;
    let mut machine_aplic_hidden = false;
    let mut thead_plic = None;
    let mut plmt = None;
    let mut plicsw = None;

    try_for_each_enabled_node(platform.root(), &mut |discovered, source_path| {
        let Some(compatibles) = discovered.compatible() else {
            return Ok(());
        };
        let node = discovered.node();
        match_desc!(first compatible in compatibles.all(); {
            plmt::COMPATIBLE => {
                describe_once(&mut plmt, || plmt::describe(platform, node))?;
            },
            plicsw::COMPATIBLE => {
                describe_once(&mut plicsw, || plicsw::describe(platform, node))?;
            },
            compatible if clint::supports(compatible) => {
                select_from_node_once(&mut clint, clint::describe(platform, node)?, source_path)?;
            },
            compatible if imsic::IMSIC_COMPATIBLES.contains(&compatible) => {
                if let Some(description) = imsic::describe(platform, node, cpu_interrupt_controllers)? {
                    select_from_node_once(&mut imsic, description, source_path)?;
                }
            },
            compatible if qemu_aplic::is_machine_domain(node, compatible) => {
                let registers = platform.device_register(node)?.ok_or(Error::InvalidArgs)?;
                select_from_node_once(&mut machine_aplic, registers, source_path)?;
                machine_aplic_hidden = qemu_aplic::is_delegated_machine_domain(node, compatible);
            },
            compatible if thead_plic::COMPATIBLES.contains(&compatible) => {
                describe_once(&mut thead_plic, || {
                    platform.device_register(node)?.ok_or(Error::InvalidArgs)
                })?;
            }
        })
    })?;

    if matches!((&plmt, &plicsw), (Some(_), Some(_))) {
        prepare_plmt(memory)?;
    }
    if let Some(registers) = thead_plic {
        thead_plic::init(registers, memory)?;
    }

    let (clint, clint_path) = match clint {
        Some(node) => {
            let (description, path) = node.into_parts();
            (Some(description), Some(path))
        }
        None => (None, None),
    };
    let clint_ipi = clint.as_ref().map(|clint| clint.ipi);
    let clint_registers = clint_ipi.map(|description| description.registers);
    let software_plic = match (plmt, plicsw) {
        (Some(plmt), Some(plicsw)) => {
            timer::init(Some(Box::new(plmt).init(memory)?))?;
            Some(plicsw)
        }
        _ => {
            let timer = clint.map(|clint| clint.timer.init(memory)).transpose()?;
            timer::init(timer)?;
            None
        }
    };

    if let Some(imsic) = imsic {
        let (imsic, imsic_path) = imsic.into_parts();
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
                    machine_aplic.as_ref().map(|node| *node.resource()),
                    clint_registers,
                    machine_base,
                    hart_index_bits,
                    imsic_end,
                    memory,
                )?;
                ipi::init(Ipi::external(device)?)?;
                hidden_node_paths.extend(clint_path);
                hidden_node_paths.push(imsic_path);
                if let Some(aplic) = machine_aplic.filter(|_| machine_aplic_hidden) {
                    hidden_node_paths.push(String::from(aplic.source_path()));
                }
                return Ok(irq_mmio);
            }
            Err(Error::NotSupported) => {}
            Err(error) => return Err(error),
        }
    }

    match (software_plic, clint_ipi) {
        (Some(plicsw), _) => {
            ipi::init(Ipi::software(Box::new(plicsw).init(memory)?)?)?;
        }
        (None, Some(description)) => {
            ipi::init(Ipi::software(Box::new(description).init(memory)?)?)?;
        }
        (None, None) => {}
    }
    Ok(None)
}

fn describe_once<T>(slot: &mut Option<T>, describe: impl FnOnce() -> Result<T>) -> Result<()> {
    if slot.is_some() {
        return Err(Error::InvalidArgs);
    }
    *slot = Some(describe()?);
    Ok(())
}
