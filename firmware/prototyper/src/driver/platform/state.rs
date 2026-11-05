//! Platform resources shared after boot-hart initialization.

use alloc::boxed::Box;
use core::sync::atomic::{AtomicBool, Ordering};

use runtime::memory::SupervisorMemory;
use spin::Once;

use super::info::BoardInfo;

static PLATFORM: Once<Platform> = Once::new();
static READY: AtomicBool = AtomicBool::new(false);

struct Platform {
    board: Box<BoardInfo>,
    supervisor_memory: SupervisorMemory,
}

/// Publishes resources constructed by the boot hart.
pub(super) fn publish_resources(board: Box<BoardInfo>, supervisor_memory: SupervisorMemory) {
    PLATFORM.call_once(|| Platform {
        board,
        supervisor_memory,
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

