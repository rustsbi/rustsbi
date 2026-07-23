//! Atomic publication state shared by cold and warm startup paths.

use core::sync::atomic::AtomicU32;

const EARLY_UNCLAIMED: u32 = 0;
pub(super) const EARLY_INITIALIZING: u32 = 1;
pub(super) const EARLY_READY: u32 = 2;
const RUNTIME_WAITING: u32 = 0;
pub(super) const RUNTIME_READY: u32 = 1;
pub(super) const RUNTIME_FAILED: u32 = 2;

#[used]
#[unsafe(link_section = ".data.entry")]
pub(super) static EARLY_STATE: AtomicU32 = AtomicU32::new(EARLY_UNCLAIMED);

#[used]
#[unsafe(link_section = ".data.entry")]
pub(super) static EARLY_FAILED: AtomicU32 = AtomicU32::new(0);

pub(super) static RUNTIME_STATE: AtomicU32 = AtomicU32::new(RUNTIME_WAITING);
