//! Startup log for the discovered platform.

use crate::cfg::NUM_HART_MAX;

use super::info::BoardInfo;
use super::state::board_info;

pub(super) fn log_platform_summary() {
    let board = board_info();

    info!("RustSBI version {}", runtime::rustsbi::VERSION);
    runtime::rustsbi::LOGO
        .lines()
        .for_each(|line| info!("{}", line));
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
    info!("{:<30}: {}", "Platform HART Count", board.harts.count);

    let mut enabled_harts = [0; NUM_HART_MAX];
    let mut count = 0;
    for (hart_id, enabled) in board.harts.enabled.iter().copied().enumerate() {
        if enabled {
            enabled_harts[count] = hart_id;
            count += 1;
        }
    }
    info!("{:<30}: {:?}", "Enabled HARTs", &enabled_harts[..count]);
}

fn log_interrupt_controller(board: &BoardInfo) {
    if crate::driver::ipi::uses_imsic()
        && let Some(imsic) = board.devices.interrupts.imsic.as_ref()
    {
        info!(
            "{:<30}: IMSIC (M-level Base Address: 0x{:x})",
            "Platform IPI Extension",
            imsic.layout.machine_base.as_usize()
        );
        return;
    }

    if let (Some(plmt), Some(plicsw)) = (
        board.devices.interrupts.plmt,
        board.devices.interrupts.plicsw,
    ) {
        info!(
            "{:<30}: Sunxi PLICSW (Base Address: 0x{:x})",
            "Platform IPI Extension",
            plicsw.start().as_usize()
        );
        info!(
            "{:<30}: Andes PLMT (Base Address: 0x{:x})",
            "Platform Timer Extension",
            plmt.start().as_usize()
        );
        return;
    }
    match board.devices.interrupts.clint.as_ref() {
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
    match board.devices.console.as_ref() {
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
    board.devices.reset.log_summary();
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
    if board.memory.ram_ranges.is_empty() {
        warn!("{:<30}: Not Available", "Platform RAM");
        return;
    }
    for (index, range) in board.memory.ram_ranges.iter().enumerate() {
        info!(
            "{:<30}: 0x{:x} - 0x{:x}",
            if index == 0 { "Platform RAM" } else { "" },
            range.start().as_usize(),
            range.end().as_usize()
        );
    }
}
