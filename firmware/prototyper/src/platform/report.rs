//! Startup log for the discovered platform.

use alloc::vec::Vec;

use super::info::BoardInfo;

pub(super) fn log_platform_summary(board: &BoardInfo) {
    info!("RustSBI version {}", runtime::rustsbi::VERSION);
    runtime::rustsbi::LOGO
        .lines()
        .for_each(|line| info!("{}", line));
    info!("Initializing RustSBI machine-mode environment.");
    info!("{:<30}: {}", "Platform Name", board.model);

    log_harts();
    super::state::interrupts().log(board);
    log_console(board);
    log_reset(board);
    log_sbi_extensions();
    log_ram(board);
}

fn log_harts() {
    info!(
        "{:<30}: {}",
        "Platform HART Count",
        runtime::hart::HartId::count()
    );

    let enabled_harts: Vec<_> = runtime::hart::HartId::all()
        .map(|hart| hart.as_usize())
        .collect();
    info!("{:<30}: {:?}", "Enabled HARTs", enabled_harts);
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
