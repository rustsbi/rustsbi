//! Atomic publication state shared by cold and warm startup paths.

use core::sync::atomic::AtomicU32;
#[cfg(not(any(feature = "jump", feature = "payload")))]
use core::sync::atomic::AtomicUsize;

const EARLY_UNCLAIMED: u32 = 0;
pub(super) const EARLY_INITIALIZING: u32 = 1;
pub(super) const EARLY_READY: u32 = 2;
const RUNTIME_WAITING: u32 = 0;
pub(super) const RUNTIME_READY: u32 = 1;
pub(super) const RUNTIME_FAILED: u32 = 2;
#[cfg(not(any(feature = "jump", feature = "payload")))]
pub(super) const DYNAMIC_WORD_COUNT: usize = 6;

#[used]
#[unsafe(link_section = ".data.entry")]
pub(super) static EARLY_STATE: AtomicU32 = AtomicU32::new(EARLY_UNCLAIMED);

#[used]
#[unsafe(link_section = ".data.entry")]
pub(super) static EARLY_FAILED: AtomicU32 = AtomicU32::new(0);

pub(super) static RUNTIME_STATE: AtomicU32 = AtomicU32::new(RUNTIME_WAITING);

#[cfg(not(any(feature = "jump", feature = "payload")))]
pub(super) static DYNAMIC_SNAPSHOT: [AtomicUsize; DYNAMIC_WORD_COUNT] =
    [const { AtomicUsize::new(0) }; DYNAMIC_WORD_COUNT];
