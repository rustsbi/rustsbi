#![forbid(unsafe_code)]

//! Safe boot entry points over the global platform state.

use core::ops::Range;
use core::sync::atomic::Ordering;

use super::{
    BOARD_INFO, BoardInfo, IS_K1_PLATFORM, IS_K3_PLATFORM, READY, board_info, print_board_info,
    publish_cpu_enabled,
};
use crate::devicetree::{Tree, parse_device_tree};
use crate::fail;
use crate::riscv::spacemit_k1;
use crate::riscv::spacemit_k3;
use crate::sbi;
use crate::sbi::SbiDispatcher;
use crate::sbi::cppc::SbiCppc;
use crate::sbi::dbtr::SbiDbtr;
use crate::sbi::fwft::SbiFwft;
use crate::sbi::hsm::SbiHsm;
use crate::sbi::nacl::SbiNacl;
use crate::sbi::rfence::SbiRFence;
use crate::sbi::sse::SbiSse;
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
    // Get other info that later platform initialization depends on.
    let cpu_list = board.discover_misc(&tree);
    publish_cpu_enabled(cpu_list);
    // Get clint and reset device, init sbi ipi, reset, hsm, rfence and susp extension.
    board.discover_devices(&root);
    let mailbox = crate::rpmi::discover_mailbox(&root);
    let ipi = sbi::ipi::init(&board);
    let hsm = ipi.as_ref().map(|_| SbiHsm);
    let rfence = ipi.as_ref().map(|_| SbiRFence);
    let susp = hsm.as_ref().map(|_| SbiSuspend);
    // Initialize pmu extension
    let pmu = sbi::pmu::init(&root);
    let sta = Some(SbiSta);
    let nacl = Some(SbiNacl);

    // Publish the board facts before platform initialization, so that
    // harts observing `READY` (Acquire) also observe the published board.
    BOARD_INFO.call_once(move || board);

    // Record platform detection before releasing the ready flag, so
    // that secondary harts observing `READY` also observe the flag.
    // Match root `compatible` strings first, then fall back to the model.
    let k3_platform = match root.get_prop("compatible") {
        Some(prop) => {
            let seq = prop.deserialize::<serde_device_tree::buildin::StrSeq>();
            spacemit_k3::is_k3_platform(&board_info().model, seq.iter())
        }
        None => spacemit_k3::is_k3_platform(&board_info().model, core::iter::empty::<&str>()),
    };
    IS_K3_PLATFORM.store(k3_platform, Ordering::Release);
    if !k3_platform {
        let k1_platform = match root.get_prop("compatible") {
            Some(prop) => {
                let seq = prop.deserialize::<serde_device_tree::buildin::StrSeq>();
                spacemit_k1::is_k1_platform(&board_info().model, seq.iter())
            }
            None => spacemit_k1::is_k1_platform(&board_info().model, core::iter::empty::<&str>()),
        };
        IS_K1_PLATFORM.store(k1_platform, Ordering::Release);
    }

    if IS_K3_PLATFORM.load(Ordering::Acquire) {
        spacemit_k3::cold_boot_allowed(crate::riscv::current_hartid());
        spacemit_k3::cold_boot_init();
        spacemit_k3::init_maplic_delegation();
        info!("SpacemiT K3: early init done (RVBADDR + CCI-550 + A100 park)");
    } else if IS_K1_PLATFORM.load(Ordering::Acquire) {
        spacemit_k1::cold_boot_init();
        info!("SpacemiT K1: early init done (MSETUP + CCI-550)");
    }

    let cppc = mailbox.and_then(|mailbox| {
        mailbox
            .probe_service_group(crate::rpmi::servicegroup::CPPC)
            .filter(|version| version >> 16 == 1)
            .map(|_| {
                let cppc = SbiCppc::new();
                cppc.set_mailbox(mailbox);
                cppc
            })
    });
    let reset = sbi::reset::init(board_info(), mailbox);
    let mpxy = mailbox.and_then(|mailbox| sbi::mpxy::SbiMpxy::from_fdt(&root, mailbox));
    // DBTR remains unavailable until trigger installation, update, and
    // lifecycle operations are implemented as a complete SBI v3.0 service.
    let dbtr: Option<SbiDbtr> = None;
    let fwft = Some(SbiFwft);
    // Keep SSE unavailable until supervisor handler context switching is implemented.
    let sse: Option<SbiSse> = None;

    sbi::SBI_DISPATCHER.call_once(|| {
        SbiDispatcher::new(
            console, cppc, dbtr, fwft, ipi, hsm, reset, rfence, susp, pmu, sta, mpxy, nacl, sse,
        )
    });

    // Secondary harts may proceed only after SoC-wide initialization and all
    // published dispatcher state are visible.
    READY.store(true, Ordering::Release);

    print_board_info();
}

/// Runs the SoC-specific per-hart setup for secondary harts.
pub fn secondary_hart_init() {
    if IS_K3_PLATFORM.load(Ordering::Acquire) {
        spacemit_k3::cold_boot_allowed(crate::riscv::current_hartid());
    } else if IS_K1_PLATFORM.load(Ordering::Acquire) {
        spacemit_k1::cold_boot_allowed(crate::riscv::current_hartid());
    }
}

/// Wakes a platform hart before HSM raises its software interrupt.
pub fn wakeup_hart(hartid: usize) {
    if IS_K3_PLATFORM.load(Ordering::Acquire) {
        spacemit_k3::wakeup_core(hartid);
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
