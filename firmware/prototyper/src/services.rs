//! Published firmware services used after boot-hart initialization.

use alloc::boxed::Box;
use core::sync::atomic::{AtomicBool, Ordering};

use runtime::{hart::HartId, ipi::IpiDelivery, memory::SupervisorMemory};
use spin::{Mutex, Once};

use crate::driver::ConsoleDevice;

use crate::info::PlatformInfo;

static STATE: Once<State> = Once::new();
static SECONDARY_INIT: Once<Box<dyn Fn() + Send + Sync>> = Once::new();
static READY: AtomicBool = AtomicBool::new(false);

struct State {
    board: Box<PlatformInfo>,
    supervisor_memory: SupervisorMemory,
    console: Option<Mutex<Box<dyn ConsoleDevice>>>,
    privilege_checked: Box<[AtomicBool]>,
    ipi_delivery: Option<IpiDelivery>,
}

/// Publishes resources constructed by the boot hart.
pub(super) fn publish_resources(
    board: Box<PlatformInfo>,
    supervisor_memory: SupervisorMemory,
    console: Option<Box<dyn ConsoleDevice>>,
    ipi_delivery: Option<IpiDelivery>,
) {
    STATE.call_once(|| State {
        board,
        supervisor_memory,
        console: console.map(Mutex::new),
        ipi_delivery,
        privilege_checked: HartId::all().map(|_| AtomicBool::new(false)).collect(),
    });
}

/// Registers the target's per-hart secondary initialization.
pub(crate) fn set_secondary_init(init: Box<dyn Fn() + Send + Sync>) {
    SECONDARY_INIT.call_once(|| init);
}

/// Runs the target's per-hart secondary initialization, if any.
pub(crate) fn initialize_secondary_hart() {
    if let Some(init) = SECONDARY_INIT.get() {
        init();
    }
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

fn state() -> &'static State {
    STATE.get().expect("BUG: services used before publication")
}

pub(crate) fn board_info() -> &'static PlatformInfo {
    &state().board
}

pub(crate) fn supervisor_memory() -> &'static SupervisorMemory {
    &state().supervisor_memory
}

pub(crate) fn console_device() -> Option<&'static Mutex<Box<dyn ConsoleDevice>>> {
    STATE.get().and_then(|state| state.console.as_ref())
}

/// Returns DT-enabled harts that have passed their privilege-mode check.
pub(crate) fn enabled_harts() -> Option<impl Iterator<Item = HartId>> {
    let state = STATE.get()?;
    Some(
        HartId::all()
            .filter(move |hart| state.privilege_checked[hart.index()].load(Ordering::Acquire)),
    )
}

/// Returns whether the target hart has passed its privilege-mode check.
pub(crate) fn hart_privilege_checked(hart_id: usize) -> bool {
    let Ok(hart) = HartId::from_raw(hart_id) else {
        return false;
    };
    STATE
        .get()
        .is_some_and(|state| state.privilege_checked[hart.index()].load(Ordering::Acquire))
}

pub(crate) fn mark_hart_privilege_checked(hart_id: usize) {
    let hart = HartId::from_raw(hart_id).expect("BUG: unknown hart ID");
    state().privilege_checked[hart.index()].store(true, Ordering::Release);
}

pub(crate) fn ipi_delivery() -> Option<IpiDelivery> {
    state().ipi_delivery
}
