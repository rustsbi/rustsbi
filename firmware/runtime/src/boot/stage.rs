//! Validated privilege choices for architectural handoff.

use csr::machine::TargetPrivilege;

/// An architectural entry with hart ID in `a0` and an opaque word in `a1`.
/// Lower-mode entries run outside the firmware's Rust execution context.
#[derive(Clone, Copy, Debug)]
pub struct NextStage {
    start_addr: usize,
    opaque: usize,
    next_mode: TargetPrivilege,
}

impl NextStage {
    /// Creates a supervisor entry with translation and interrupts disabled.
    pub const fn supervisor(start_addr: usize, opaque: usize) -> Self {
        Self {
            start_addr,
            opaque,
            next_mode: TargetPrivilege::Supervisor,
        }
    }

    /// Creates a user-mode entry with translation and interrupts disabled.
    pub const fn user(start_addr: usize, opaque: usize) -> Self {
        Self {
            start_addr,
            opaque,
            next_mode: TargetPrivilege::User,
        }
    }

    /// Creates a loader-selected entry, including an M-mode entry.
    ///
    /// # Safety
    ///
    /// An M-mode target must accept the architectural handoff on every hart
    /// that can consume this value, with no Rust stack and only `a0`/`a1` defined.
    /// Its entry code must establish its own Rust calling context and preserve
    /// live firmware allocations and references. These requirements must remain
    /// valid for every copy of the stage, including the supplied opaque word.
    pub const unsafe fn new(start_addr: usize, opaque: usize, next_mode: TargetPrivilege) -> Self {
        Self {
            start_addr,
            opaque,
            next_mode,
        }
    }

    /// Returns the architectural entry address.
    pub const fn start_addr(self) -> usize {
        self.start_addr
    }

    /// Returns the word passed in `a1`.
    pub const fn opaque(self) -> usize {
        self.opaque
    }

    /// Returns the selected privilege mode.
    pub const fn next_mode(self) -> TargetPrivilege {
        self.next_mode
    }
}
