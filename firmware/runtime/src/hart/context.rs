//! Boot-allocated Runtime contexts shared by hart lifecycle and trap handling.

use alloc::boxed::Box;
use alloc::vec::Vec;
use core::fmt;
use core::sync::atomic::{AtomicBool, AtomicU8};

use rustsbi::RustSBI;
use spin::Once;

use super::HartId;
use super::lifecycle::HartStateCell;

/// Trap activation is independent of the hart's start/stop state.
#[repr(u8)]
pub(crate) enum TrapPhase {
    Uninitialized,
    Initializing,
    Ready,
    Armed,
}

pub(crate) struct TrapStateCell {
    pub(crate) phase: AtomicU8,
    pub(crate) policy: Once<&'static (dyn RustSBI + Sync)>,
    pub(crate) has_sstc: AtomicBool,
}

pub(crate) struct HartContext {
    pub(super) lifecycle: HartStateCell,
    pub(crate) trap: TrapStateCell,
}

impl HartContext {
    const fn new() -> Self {
        Self {
            lifecycle: HartStateCell::new(),
            trap: TrapStateCell {
                phase: AtomicU8::new(TrapPhase::Uninitialized as u8),
                policy: Once::new(),
                has_sstc: AtomicBool::new(false),
            },
        }
    }
}

struct Harts {
    ids: Box<[usize]>,
    contexts: Box<[HartContext]>,
}

static HARTS: Once<Harts> = Once::new();

/// Failure to publish the boot-discovered Runtime hart contexts.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HartInitError {
    /// The Runtime hart contexts have already been published.
    AlreadyInitialized,
}

impl fmt::Display for HartInitError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::AlreadyInitialized => "Runtime hart contexts already initialized",
        })
    }
}

/// Allocates and publishes one Runtime context per boot-discovered hart.
///
/// Call this on the boot hart after initializing the allocator and discovering
/// the topology, before staging a handoff, activating traps, or releasing
/// secondary harts. Include harts that are still held in hardware reset.
/// The ID table must be nonempty; identifiers may be unsorted or repeated.
/// Initialization sorts and deduplicates them before allocating contexts.
/// Published contexts remain allocated across hart stop, start, and suspend
/// transitions.
pub fn init(mut ids: Vec<usize>) -> Result<(), HartInitError> {
    let mut initialized = false;
    HARTS.call_once(|| {
        ids.sort_unstable();
        ids.dedup();
        let ids = ids.into_boxed_slice();
        let contexts = ids.iter().map(|_| HartContext::new()).collect();
        initialized = true;
        Harts { ids, contexts }
    });
    if initialized {
        Ok(())
    } else {
        Err(HartInitError::AlreadyInitialized)
    }
}

pub(crate) fn get(hart: HartId) -> Option<&'static HartContext> {
    let table = HARTS.get()?;
    let index = table.ids.binary_search(&hart.as_usize()).ok()?;
    Some(&table.contexts[index])
}
