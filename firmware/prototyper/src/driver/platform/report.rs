//! Startup log for the discovered platform.

use runtime::{
    hart::HartId,
    memory::PhysAddrRange,
    rustsbi::{LOGO, VERSION},
};

pub(super) fn log_platform_summary(
    model: &str,
    timebase_frequency_hz: Option<u32>,
    ram_ranges: impl Iterator<Item = PhysAddrRange>,
) {
    info!("RustSBI version {}", VERSION);
    LOGO.lines().for_each(|line| info!("{}", line));
    info!("Initializing RustSBI machine-mode environment.");
    info!("{:<30}: {}", "Platform Name", model);
    if let Some(frequency) = timebase_frequency_hz {
        info!("{:<30}: {} Hz", "Platform Timebase Frequency", frequency);
    }
    info!("{:<30}: {}", "Platform HART Count", HartId::count());
    for (index, range) in ram_ranges.enumerate() {
        info!(
            "{:<30}: 0x{:x} - 0x{:x}",
            if index == 0 { "Platform RAM" } else { "" },
            range.start().as_usize(),
            range.end().as_usize()
        );
    }
}
