//! Prototyper's loader decoding and bootstrap publication.

use super::BootInfo;
use crate::{cfg, heap};
use runtime::{
    DeviceTreeHandoff,
    boot::{BootStack, initialize_stacks},
};
use spin::{Mutex, Once};

static BOOT_INFO: Mutex<Option<BootInfo>> = Mutex::new(None);
static BOOT_HART: Once<usize> = Once::new();

/// Initializes the selected boot hart under Runtime's bootstrap lock.
///
/// # Safety
/// Runtime startup must own the bootstrap stack, exclude interrupts, and keep
/// secondary harts out until stack initialization and this Rust frame retire.
/// The loader must satisfy Runtime's entry storage and lifetime requirements.
pub(crate) unsafe fn initialize(
    hart_id: usize,
    device_tree: DeviceTreeHandoff,
    dynamic_info_address: usize,
    stack: &'static BootStack<{ cfg::STACK_SIZE_PER_HART }>,
) -> usize {
    let boot = BootInfo::decode(hart_id, device_tree, dynamic_info_address)
        .expect("firmware entry rejected the platform description");
    if !boot.is_boot_hart() {
        return 0;
    }
    heap::init();
    // SAFETY: Runtime's bootstrap lock serializes candidates before publication;
    // the loader grants exclusive RAM for the firmware and discovered stacks.
    unsafe {
        initialize_stacks(
            stack,
            cfg::HART_CAPACITY,
            boot.platform_description(),
            boot.next_stage().start_addr(),
            boot.dynamic_info_range()
                .expect("invalid dynamic boot information range"),
        )
    }
    .expect("firmware could not initialize its hart topology");
    BOOT_HART.call_once(|| hart_id);
    *BOOT_INFO.lock() = Some(boot);
    1
}

/// Takes the elected boot handoff or decodes this secondary hart's envelope.
pub(crate) fn boot_info(
    hart_id: usize,
    device_tree: DeviceTreeHandoff,
    dynamic_info: usize,
) -> BootInfo {
    if BOOT_HART.get().copied() == Some(hart_id) {
        BOOT_INFO.lock().take().expect("boot handoff consumed once")
    } else {
        BootInfo::secondary(device_tree, dynamic_info)
    }
}
