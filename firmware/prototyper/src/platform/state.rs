//! Platform resources shared after boot-hart initialization.

use alloc::boxed::Box;
use core::sync::atomic::{AtomicBool, Ordering};

use runtime::SpacemitK1Registers;
use runtime::hart::HartId;
use runtime::memory::{PhysAddrRange, SupervisorMemory};
use spin::Once;

use crate::driver::Console;

use super::{
    devices::Devices,
    info::{BoardInfo, SocDescription},
    interrupts::InterruptController,
};

static PLATFORM: Once<Platform> = Once::new();
static READY: AtomicBool = AtomicBool::new(false);

pub(super) struct Platform {
    pub(super) firmware_ram_range: PhysAddrRange,
    pub(super) noncacheable_alias_offset: Option<u64>,
    pub(super) secondary_hart: Option<SpacemitK1Registers>,
    supervisor_memory: SupervisorMemory,
    devices: Devices,
    privilege_checked: Box<[AtomicBool]>,
}

/// Publishes resources constructed by the boot hart.
pub(super) fn publish_resources(
    board: &BoardInfo,
    supervisor_memory: SupervisorMemory,
    devices: Devices,
) -> runtime::Result<()> {
    let mut published = false;
    PLATFORM.call_once(|| {
        published = true;
        Platform {
            firmware_ram_range: board
                .memory
                .firmware_ram_range
                .expect("BUG: firmware RAM bank missing after platform initialization"),
            noncacheable_alias_offset: board.memory.noncacheable_alias_offset,
            secondary_hart: match &board.soc {
                Some(SocDescription::SpacemitK1(registers)) => Some(*registers),
                _ => None,
            },
            supervisor_memory,
            devices,
            privilege_checked: HartId::all().map(|_| AtomicBool::new(false)).collect(),
        }
    });
    if published {
        Ok(())
    } else {
        Err(runtime::Error::AlreadyInitialized)
    }
}

/// Releases secondary harts after all published services are ready.
pub(super) fn mark_ready() {
    READY.store(true, Ordering::Release);
}

/// Spins until the boot hart has finished platform initialization.
pub(crate) fn wait_until_ready() {
    while !READY.load(Ordering::Acquire) {
        core::hint::spin_loop()
    }
}

pub(super) fn platform() -> &'static Platform {
    PLATFORM
        .get()
        .expect("BUG: platform resources used before publication")
}

pub(crate) fn supervisor_memory() -> &'static SupervisorMemory {
    &platform().supervisor_memory
}

pub(super) fn devices() -> &'static Devices {
    &platform().devices
}

pub(crate) fn console_device() -> Option<&'static Console> {
    PLATFORM
        .get()
        .and_then(|platform| platform.devices.console.as_ref())
}

pub(crate) fn time_source() -> Option<&'static dyn runtime::timer::TimeSource> {
    PLATFORM
        .get()
        .and_then(|platform| platform.devices.interrupts.timer())
        .and_then(runtime::timer::TimerDevice::time_source)
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

/// Borrows the owned interrupt devices and their fixed next-stage policy.
pub(crate) fn interrupts() -> &'static InterruptController {
    &platform().devices.interrupts
}
