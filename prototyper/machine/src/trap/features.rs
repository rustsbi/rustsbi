//! Per-hart architectural facts used by closed trap routing.

use core::sync::atomic::{AtomicBool, Ordering};

use crate::config::HART_CAPACITY;

static HYPERVISOR_METADATA: [AtomicBool; HART_CAPACITY] =
    [const { AtomicBool::new(false) }; HART_CAPACITY];

pub(crate) fn enable_hypervisor_metadata(index: usize) -> bool {
    let Some(available) = HYPERVISOR_METADATA.get(index) else {
        return false;
    };
    available.store(true, Ordering::Release);
    true
}

pub(crate) fn hypervisor_metadata_available(index: usize) -> bool {
    HYPERVISOR_METADATA
        .get(index)
        .is_some_and(|available| available.load(Ordering::Acquire))
}
