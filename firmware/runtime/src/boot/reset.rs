//! Shared lifecycle for harts entering through a SoC reset vector.
//!
//! Each SoC installs its own stackless entry. After restoring coherent memory
//! access and selecting a Runtime stack, that entry calls the initializer
//! registered here. The initializer must activate traps before it returns.

use core::fmt;

use spin::Once;

use crate::memory::PhysAddr;

static INITIALIZER: Once<fn()> = Once::new();

/// A registered hardware entry for a hart released from reset.
///
/// Only a validated SoC can select this entry. It is a physical reset-vector
/// address, not a callable Rust function. Publish platform services and enable
/// cluster coherency before releasing a hart into it.
pub struct ResetEntry(PhysAddr);

impl ResetEntry {
    /// Returns the physical entry address to encode in reset-vector registers.
    pub const fn address(&self) -> PhysAddr {
        self.0
    }

    /// Registers the shared initializer for one Runtime-owned SoC entry.
    ///
    /// The caller must select a fixed entry that restores coherent memory and
    /// selects a Runtime stack before calling [`initialize_reset_hart`].
    pub(crate) fn register(
        entry: unsafe extern "C" fn() -> !,
        initialize: fn(),
    ) -> Result<Self, ResetEntryAlreadyRegistered> {
        let mut registered = false;
        INITIALIZER.call_once(|| {
            registered = true;
            initialize
        });
        if !registered {
            return Err(ResetEntryAlreadyRegistered);
        }
        Ok(Self(PhysAddr::new(entry as *const () as usize)))
    }
}

/// A hardware reset initializer has already been registered.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResetEntryAlreadyRegistered;

impl fmt::Display for ResetEntryAlreadyRegistered {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("hardware reset initializer already registered")
    }
}

/// Enters shared policy after the SoC entry has established a coherent stack.
pub(crate) extern "C" fn initialize_reset_hart() -> ! {
    let Some(initialize) = INITIALIZER.get() else {
        fail_stop()
    };
    initialize();
    if !crate::trap::init::current_is_ready() || riscv::register::mstatus::read().mie() {
        fail_stop();
    }
    // SAFETY:
    // 1. The SoC entry established M-mode and selected this hart's published stack.
    // 2. The checks above confirm ready traps and MIE clear after policy returns.
    // 3. The initializer's call chain is complete and can be discarded.
    unsafe { super::finish_boot() }
}

fn fail_stop() -> ! {
    // SAFETY: the SoC entry established M-mode; fail_stop needs no stack.
    unsafe { super::fail_stop() }
}
