//! Platform resources shared after boot-hart initialization.

use alloc::boxed::Box;
use core::sync::atomic::{AtomicBool, Ordering};

use runtime::hart::HartId;
use runtime::memory::SupervisorMemory;
use spin::Once;

use crate::driver::Console;

use super::info::BoardInfo;

static PLATFORM: Once<Platform> = Once::new();
static READY: AtomicBool = AtomicBool::new(false);

struct Platform {
    board: Box<BoardInfo>,
    supervisor_memory: SupervisorMemory,
    console: Option<Console>,
    privilege_checked: Box<[AtomicBool]>,
}

/// Publishes resources constructed by the boot hart.
pub(super) fn publish_resources(
    board: Box<BoardInfo>,
    supervisor_memory: SupervisorMemory,
    console: Option<Console>,
) {
    PLATFORM.call_once(|| Platform {
        board,
        supervisor_memory,
        console,
        privilege_checked: HartId::all().map(|_| AtomicBool::new(false)).collect(),
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

pub(crate) fn console_device() -> Option<&'static Console> {
    PLATFORM
        .get()
        .and_then(|platform| platform.console.as_ref())
}

/// Returns DT-enabled harts that have passed their privilege-mode check.
pub(crate) fn enabled_harts() -> Option<impl Iterator<Item = HartId>> {
    let platform = PLATFORM.get()?;
    Some(
        HartId::all()
            .filter(move |hart| platform.privilege_checked[hart.index()].load(Ordering::Acquire)),
    )
}

/// Returns whether the target hart has passed its privilege-mode check.
pub(crate) fn hart_privilege_checked(hart_id: usize) -> bool {
    let Ok(hart) = HartId::from_raw(hart_id) else {
        return false;
    };
    PLATFORM
        .get()
        .is_some_and(|platform| platform.privilege_checked[hart.index()].load(Ordering::Acquire))
}

pub(crate) fn mark_hart_privilege_checked(hart_id: usize) {
    let hart = HartId::from_raw(hart_id).expect("BUG: unknown hart ID");
    platform().privilege_checked[hart.index()].store(true, Ordering::Release);
}
