//! Per-hart firmware memory protection and PMP reporting.

use core::ops::Range;
use runtime::{
    ipi::IpiDelivery,
    memory::{FirmwareProtection, PhysAddr, PhysAddrRange},
};

const QEMU_VIRT_CLINT_BASE: usize = 0x0200_0000;
const QEMU_VIRT_CLINT_SIZE: usize = 0x1_0000;

/// Selects the firmware protection layout from the board description.
///
/// QEMU AIA keeps its machine-level interrupt controllers inaccessible to
/// S-mode. Other boards protect firmware RAM and may deny the firmware's
/// non-cacheable alias.
pub fn protection(firmware_ram: &Range<usize>) -> FirmwareProtection {
    let firmware_ram = PhysAddrRange::new(
        PhysAddr::new(firmware_ram.start),
        PhysAddr::new(firmware_ram.end),
    )
    .expect("BUG: firmware RAM range is empty or reversed");
    let board = crate::platform::board_info();
    let mut denied = alloc::vec::Vec::new();

    if matches!(
        crate::platform::ipi_delivery(),
        Some(IpiDelivery::Imsic { .. })
    ) && board.is_qemu_virt()
        && let Some(imsic) = board.devices.interrupts.imsic()
    {
        let clint_start = board
            .devices
            .interrupts
            .clint()
            .map(|description| description.resource().registers.start().as_usize())
            .unwrap_or(QEMU_VIRT_CLINT_BASE);
        let clint_end = clint_start
            .checked_add(QEMU_VIRT_CLINT_SIZE)
            .expect("BUG: QEMU CLINT window overflowed");
        let aplic = board
            .devices
            .interrupts
            .machine_aplic()
            .expect("BUG: QEMU AIA setup requires a machine APLIC");
        let machine_imsic_start = imsic.resource().layout.machine_base.as_usize();
        let machine_imsic_end = imsic
            .resource()
            .hart_files
            .iter()
            .map(|range| range.end().as_usize())
            .max()
            .expect("BUG: every enabled hart requires an IMSIC file");

        denied.push(window(clint_start, clint_end));
        denied.push(window(
            aplic.resource().start().as_usize(),
            aplic.resource().end().as_usize(),
        ));
        denied.push(window(machine_imsic_start, machine_imsic_end));
        FirmwareProtection::new(firmware_ram, denied, None)
            .expect("BUG: QEMU AIA firmware protection layout is invalid")
    } else {
        FirmwareProtection::new(firmware_ram, denied, board.memory.noncacheable_alias_offset)
            .expect("BUG: firmware protection layout is invalid")
    }
}

/// Installs PMP entries isolating firmware memory from S-mode.
pub fn set_pmp(protection: &FirmwareProtection) {
    protection
        .install()
        .expect("failed to install firmware PMP protection");
}

/// Logs the active PMP configuration.
pub fn log_pmp_cfg(protection: &FirmwareProtection) {
    info!("PMP Configuration");

    info!(
        "{:<5} {:<10} {:<15} {:<30}",
        "PMP", "Range", "Permission", "Address"
    );

    for entry in protection
        .read()
        .expect("failed to read firmware PMP protection")
    {
        info!(
            "{:<5} {:<10} {:<15} 0x{:016x}",
            entry.index(),
            entry.range(),
            entry.permission(),
            (entry.address() as u64) << 2,
        );
    }
}

fn window(start: usize, end: usize) -> PhysAddrRange {
    PhysAddrRange::new(PhysAddr::new(start), PhysAddr::new(end))
        .expect("BUG: denied firmware window is empty or reversed")
}
