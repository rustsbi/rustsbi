//! Platform resources shared after boot-hart initialization.

use alloc::boxed::Box;
use core::sync::atomic::{AtomicBool, Ordering};

use runtime::memory::SupervisorMemory;
use spin::{Mutex, Once, RwLock};

use crate::cfg::NUM_HART_MAX;
use crate::driver::ConsoleDevice;

use super::info::{BoardInfo, HartEnableList};

static PLATFORM: Once<Platform> = Once::new();
static ENABLED_HARTS: RwLock<Option<HartEnableList>> = RwLock::new(None);
static HART_PRIVILEGE_CHECKED: [AtomicBool; NUM_HART_MAX] =
    [const { AtomicBool::new(false) }; NUM_HART_MAX];
static READY: AtomicBool = AtomicBool::new(false);

struct Platform {
    board: BoardInfo,
    supervisor_memory: SupervisorMemory,
    console: Option<Mutex<Box<dyn ConsoleDevice>>>,
}

/// Publishes resources constructed by the boot hart.
pub(super) fn publish_resources(
    board: BoardInfo,
    supervisor_memory: SupervisorMemory,
    console: Option<Box<dyn ConsoleDevice>>,
) {
    let enabled_harts = board.enabled_harts;
    PLATFORM.call_once(|| Platform {
        board,
        supervisor_memory,
        console: console.map(Mutex::new),
    });
    *ENABLED_HARTS.write() = Some(enabled_harts);
}

/// Releases secondary harts after all published services are ready.
pub(super) fn mark_ready() {
    READY.store(true, Ordering::Release);
}

pub(super) fn wait_until_ready() {
    while !READY.load(Ordering::Acquire) {
        core::hint::spin_loop()
    }
}

fn platform() -> &'static Platform {
    PLATFORM
        .get()
        .expect("BUG: platform resources used before publication")
}

pub(crate) fn board_info() -> &'static BoardInfo {
    &platform().board
}

pub(crate) fn supervisor_memory() -> &'static SupervisorMemory {
    &platform().supervisor_memory
}

pub(crate) fn console_device() -> Option<&'static Mutex<Box<dyn ConsoleDevice>>> {
    PLATFORM
        .get()
        .and_then(|platform| platform.console.as_ref())
}

pub(crate) fn enabled_harts() -> Option<HartEnableList> {
    *ENABLED_HARTS.read()
}

pub(crate) fn mark_hart_privilege_checked(hart_id: usize) {
    HART_PRIVILEGE_CHECKED
        .get(hart_id)
        .expect("BUG: hart ID exceeds the configured limit")
        .store(true, Ordering::Release);
}

/// Removes harts that failed the per-hart privilege-mode check.
pub(crate) fn retain_privilege_checked_harts() {
    let mut enabled_harts = ENABLED_HARTS.write();
    let Some(enabled_harts) = enabled_harts.as_mut() else {
        return;
    };
    for (hart_id, enabled) in enabled_harts.iter_mut().enumerate() {
        if *enabled {
            *enabled = HART_PRIVILEGE_CHECKED[hart_id].load(Ordering::Acquire);
        }
    }
}
