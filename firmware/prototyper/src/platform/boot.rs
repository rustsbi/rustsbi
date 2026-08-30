#![forbid(unsafe_code)]

//! Safe boot entry points over the global platform state.

use core::ops::Range;
use core::sync::atomic::Ordering;

use super::{
    BOARD_INFO, BoardInfo, IS_K1_PLATFORM, READY, board_info, print_board_info, publish_cpu_enabled,
};
use crate::devicetree::{Tree, parse_device_tree};
use crate::fail;
use crate::riscv::spacemit_k1;
use crate::sbi;
use crate::sbi::SbiDispatcher;
use crate::sbi::cppc::SbiCppc;
use crate::sbi::hsm::SbiHsm;
use crate::sbi::rfence::SbiRFence;
use crate::sbi::sta::SbiSta;
use crate::sbi::suspend::SbiSuspend;

/// Initializes the board from the device tree and runs the SoC-specific
/// early initialization.
pub fn init_board(fdt_address: usize) {
    let dtb = parse_device_tree(fdt_address).unwrap_or_else(fail::device_tree_format);
    let dtb = dtb.share();

    let root: serde_device_tree::buildin::Node =
        serde_device_tree::from_raw_mut(&dtb).unwrap_or_else(fail::device_tree_deserialize_root);
    let tree: Tree = root.deserialize();

    let mut board = BoardInfo::new();
    // Get console device, init sbi console and logger.
    board.discover_console(&root);
    let console = sbi::console::init(&board);
    let cppc = Some(SbiCppc::new());
    // Get other info that later platform initialization depends on.
    let cpu_list = board.discover_misc(&tree);
    publish_cpu_enabled(cpu_list);
    // Get clint and reset device, init sbi ipi, reset, hsm, rfence and susp extension.
    board.discover_devices(&root);
    let ipi = sbi::ipi::init(&board);
    let hsm = ipi.as_ref().map(|_| SbiHsm);
    let reset = sbi::reset::init(&board);
    let rfence = ipi.as_ref().map(|_| SbiRFence);
    let susp = hsm.as_ref().map(|_| SbiSuspend);
    // Initialize pmu extension
    let pmu = sbi::pmu::init(&root);
    let mpxy = Some(sbi::mpxy::SbiMpxy::new());
    let sta = Some(SbiSta);

    // Publish the SBI extension set before the K1 detect / READY release,
    // so that harts observing `READY` also observe the published dispatcher.
    sbi::SBI_DISPATCHER.call_once(|| {
        SbiDispatcher::new(console, cppc, ipi, hsm, reset, rfence, susp, pmu, sta, mpxy)
    });

    // Publish the board facts before the K1 detect / READY release, so that
    // harts observing `READY` (Acquire) also observe the published board.
    BOARD_INFO.call_once(move || board);

    // Record K1 platform detection *before* releasing the ready flag, so
    // that secondary harts observing `READY` also observe the flag.
    // Match the root node's `compatible` strings first (OpenSBI's
    // spacemit_k1_match[] table), falling back to the model string.
    let k1_platform = match root.get_prop("compatible") {
        Some(prop) => {
            let seq = prop.deserialize::<serde_device_tree::buildin::StrSeq>();
            spacemit_k1::is_k1_platform(&board_info().model, seq.iter())
        }
        None => spacemit_k1::is_k1_platform(&board_info().model, core::iter::empty::<&str>()),
    };
    IS_K1_PLATFORM.store(k1_platform, Ordering::Release);

    READY.store(true, Ordering::Release);

    print_board_info();

    if IS_K1_PLATFORM.load(Ordering::Acquire) {
        spacemit_k1::cold_boot_init();
        info!("SpacemiT K1: early init done (MSETUP + CCI-550)");
    }
}

/// Runs the SoC-specific per-hart setup for secondary harts.
pub fn secondary_hart_init() {
    if IS_K1_PLATFORM.load(Ordering::Acquire) {
        spacemit_k1::cold_boot_allowed(crate::riscv::current_hartid());
    }
}

/// Spins until the boot hart has finished platform initialization.
pub fn wait_until_ready() {
    while !READY.load(Ordering::Acquire) {
        core::hint::spin_loop()
    }
}

/// Returns the board's memory range (set during `init_board`).
pub fn memory_range() -> Range<usize> {
    board_info().memory_range.as_ref().unwrap().clone()
}
