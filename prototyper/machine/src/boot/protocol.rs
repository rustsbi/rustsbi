//! Validated next-stage entry facts shared by boot providers.

use super::BootDtbImportError;

/// A validated privilege transition prepared from the selected boot protocol.
///
/// Its raw privilege representation remains private. A supervisor HSM target
/// can be created only through the checked constructor.
pub struct NextStage {
    pub(super) entry: usize,
    pub(super) opaque: usize,
    pub(super) mode: NextMode,
}

impl NextStage {
    pub(super) const fn new(entry: usize, opaque: usize, mode: NextMode) -> Self {
        Self {
            entry,
            opaque,
            mode,
        }
    }

    /// Validates a supervisor entry address and its opaque ABI argument.
    pub fn supervisor(entry: usize, opaque: usize) -> Result<Self, crate::HartError> {
        if !crate::config::next_address_allowed(entry) || !entry.is_multiple_of(2) {
            return Err(crate::HartError::InvalidAddress);
        }
        Ok(Self {
            entry,
            opaque,
            mode: NextMode::Supervisor,
        })
    }

    pub(crate) fn into_parts(self) -> (usize, usize, NextMode) {
        (self.entry, self.opaque, self.mode)
    }

    pub(crate) const fn mode(&self) -> NextMode {
        self.mode
    }

    pub(super) fn invariants_hold(&self) -> bool {
        self.entry != 0
            && matches!(
                self.mode,
                NextMode::User | NextMode::Supervisor | NextMode::Machine
            )
    }

    #[cfg(test)]
    pub(crate) fn for_test(entry: usize) -> Self {
        Self {
            entry,
            opaque: 0,
            mode: NextMode::Supervisor,
        }
    }
}

/// A validated next-stage privilege mode.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NextMode {
    /// User mode.
    User,
    /// Supervisor mode.
    Supervisor,
    /// Machine mode.
    Machine,
}

impl NextMode {
    /// Returns the standard RISC-V privilege encoding used by `mstatus.MPP`.
    pub(super) const fn privilege_encoding(self) -> usize {
        match self {
            Self::User => 0,
            Self::Supervisor => 1,
            Self::Machine => 3,
        }
    }
}

const _: () = {
    assert!(NextMode::User.privilege_encoding() == 0);
    assert!(NextMode::Supervisor.privilege_encoding() == 1);
    assert!(NextMode::Machine.privilege_encoding() == 3);
};

/// Rejection reasons while constructing complete owned boot input.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BootInfoError {
    /// The selected boot provider supplied an invalid protocol envelope.
    InvalidBootProtocol,
    /// The DTB envelope or owned-copy operation was rejected.
    Dtb(BootDtbImportError),
    /// An internal owned-state invariant was not established.
    InvalidOwnedState,
}

impl From<BootDtbImportError> for BootInfoError {
    fn from(error: BootDtbImportError) -> Self {
        Self::Dtb(error)
    }
}
