//! Startup log for the discovered platform.

use crate::cfg::NUM_HART_MAX;

use super::info::BoardInfo;
use super::state::{board_info, enabled_harts};

pub(super) fn log_platform_summary() {
    let board = board_info();

    info!("RustSBI version {}", rustsbi::VERSION);
    rustsbi::LOGO.lines().for_each(|line| info!("{}", line));
    info!("Initializing RustSBI machine-mode environment.");
    info!("{:<30}: {}", "Platform Name", board.model);

    log_harts(board);
    log_interrupt_controller(board);
    log_console(board);
    log_reset(board);
    log_sbi_extensions();
    log_ram(board);
}

fn log_harts(board: &BoardInfo) {
    info!("{:<30}: {}", "Platform HART Count", board.hart_count);

    let Some(enabled) = enabled_harts() else {
        warn!("{:<30}: Not Available", "Enabled HARTs");
        return;
    };
    let mut enabled_harts = [0; NUM_HART_MAX];
    let mut count = 0;
    for (hart_id, enabled) in enabled.iter().copied().enumerate() {
        if enabled {
            enabled_harts[count] = hart_id;
            count += 1;
        }
    }
    info!("{:<30}: {:?}", "Enabled HARTs", &enabled_harts[..count]);
}

fn log_interrupt_controller(board: &BoardInfo) {
    if crate::sbi::ipi::uses_imsic()
        && let Some(imsic) = board.imsic.as_ref()
    {
        info!(
            "{:<30}: IMSIC (M-level Base Address: 0x{:x})",
            "Platform IPI Extension",
            imsic.layout.machine_base.as_usize()
        );
        return;
    }

    match board.clint.as_ref() {
        Some((registers, kind)) => info!(
            "{:<30}: {} (Base Address: 0x{:x})",
            "Platform IPI Extension",
            kind.name(),
            registers.start().as_usize()
        ),
        None => warn!("{:<30}: Not Available", "Platform IPI Device"),
    }
}

fn log_console(board: &BoardInfo) {
    match board.console.as_ref() {
        Some(console) => info!(
            "{:<30}: {} (Base Address: 0x{:x})",
            "Platform Console Extension",
            console.kind.name(),
            console.registers.start().as_usize()
        ),
        None => warn!("{:<30}: Not Available", "Platform Console Device"),
    }
}

fn log_reset(board: &BoardInfo) {
    if let Some(registers) = board.reset {
        info!(
            "{:<30}: Available (Base Address: 0x{:x})",
            "Platform Reset Extension",
            registers.start().as_usize()
        );
    } else if let Some((controller, address)) = board.pmic_reset {
        info!(
            "{:<30}: Available (P1 PMIC @ 0x{:02x}, I2C Base: 0x{:x})",
            "Platform Reset Extension",
            address.get(),
            controller.start().as_usize()
        );
    } else {
        warn!("{:<30}: Not Available", "Platform Reset Device");
    }
}

fn log_sbi_extensions() {
    log_availability("Platform HSM Extension", crate::sbi::hsm().is_some());
    log_availability("Platform RFence Extension", crate::sbi::rfence().is_some());
    log_availability("Platform SUSP Extension", crate::sbi::susp().is_some());
    log_availability("Platform PMU Extension", crate::sbi::pmu().is_some());
}

fn log_availability(name: &str, available: bool) {
    if available {
        info!("{name:<30}: Available");
    } else {
        warn!("{name:<30}: Not Available");
    }
}

fn log_ram(board: &BoardInfo) {
    if board.ram_ranges.is_empty() {
        warn!("{:<30}: Not Available", "Platform RAM");
        return;
    }
    for (index, range) in board.ram_ranges.iter().enumerate() {
        info!(
            "{:<30}: 0x{:x} - 0x{:x}",
            if index == 0 { "Platform RAM" } else { "" },
            range.start().as_usize(),
            range.end().as_usize()
        );
    }
}
