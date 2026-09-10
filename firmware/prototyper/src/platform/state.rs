//! Platform resources shared after boot-hart initialization.

use alloc::boxed::Box;
use core::sync::atomic::{AtomicBool, Ordering};

use runtime::memory::SupervisorMemory;
use spin::{Mutex, Once};

use crate::cfg::NUM_HART_MAX;
use crate::driver::DbcnBackend;

use super::info::{BoardInfo, HartEnableList};

static PLATFORM: Once<Platform> = Once::new();
static HART_PRIVILEGE_CHECKED: [AtomicBool; NUM_HART_MAX] =
    [const { AtomicBool::new(false) }; NUM_HART_MAX];
static READY: AtomicBool = AtomicBool::new(false);

struct Platform {
    board: BoardInfo,
    supervisor_memory: SupervisorMemory,
    console: Option<Mutex<Box<dyn DbcnBackend + Send>>>,
}

/// Publishes resources constructed by the boot hart.
pub(super) fn publish_resources(
    board: BoardInfo,
    supervisor_memory: SupervisorMemory,
    console: Option<Box<dyn DbcnBackend + Send>>,
) {
    PLATFORM.call_once(|| Platform {
        board,
        supervisor_memory,
        console: console.map(Mutex::new),
    });
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

pub(crate) fn console_device() -> Option<&'static Mutex<Box<dyn DbcnBackend + Send>>> {
    PLATFORM
        .get()
        .and_then(|platform| platform.console.as_ref())
}

/// Returns DT-enabled harts that have passed their privilege-mode check.
pub(crate) fn enabled_harts() -> Option<HartEnableList> {
    let mut enabled = PLATFORM.get()?.board.enabled_harts;
    for (enabled, checked) in enabled.iter_mut().zip(&HART_PRIVILEGE_CHECKED) {
        *enabled &= checked.load(Ordering::Acquire);
    }
    Some(enabled)
}

/// Returns whether the target hart has passed its privilege-mode check.
pub(crate) fn hart_privilege_checked(hart_id: usize) -> bool {
    HART_PRIVILEGE_CHECKED
        .get(hart_id)
        .is_some_and(|checked| checked.load(Ordering::Acquire))
}

pub(crate) fn mark_hart_privilege_checked(hart_id: usize) {
    HART_PRIVILEGE_CHECKED
        .get(hart_id)
        .expect("BUG: hart ID exceeds the configured limit")
        .store(true, Ordering::Release);
}
