//! Validation of previous-stage boot protocols and next-stage entry facts.

use super::BootDtbImportError;

pub(crate) const DYNAMIC_MAGIC: usize = 0x4942_534f;

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
    pub(super) fn from_dynamic(boot: DynamicBoot) -> Self {
        Self {
            entry: boot.next_address,
            opaque: 0,
            mode: boot.next_mode,
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

/// The six XLEN-sized words copied from a dynamic firmware handoff.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DynamicWords {
    /// Dynamic-information magic.
    pub magic: usize,
    /// Dynamic-information ABI version.
    pub version: usize,
    /// Next-stage entry address.
    pub next_address: usize,
    /// Encoded next-stage privilege mode.
    pub next_mode: usize,
    /// Provider-defined option bits.
    pub options: usize,
    /// Preferred initialization hart, or `usize::MAX` when absent.
    pub boot_hart: usize,
}

/// How the unique initialization hart is selected.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InitHart {
    /// The named physical hart is the only allowed initializer.
    Explicit(usize),
    /// The first arriving hart wins one atomic lottery.
    FirstArrival,
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

/// Validated dynamic boot facts, free of machine-state authority.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DynamicBoot {
    /// Initialization-hart selection rule.
    pub init_hart: InitHart,
    /// Next-stage entry address.
    pub next_address: usize,
    /// Next-stage privilege mode.
    pub next_mode: NextMode,
    /// Provider-defined option bits.
    pub options: usize,
}

/// Rejection reasons for a dynamic firmware handoff.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DynamicBootError {
    /// The magic value does not identify the selected ABI.
    BadMagic,
    /// The ABI version has no selected interpretation.
    UnsupportedVersion,
    /// The next-stage address lies outside configured intervals.
    InvalidNextAddress,
    /// The next-stage privilege encoding is not supported.
    InvalidNextMode,
}

/// Rejection reasons while constructing complete owned boot input.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BootInfoError {
    /// The dynamic-information envelope was rejected.
    Dynamic(DynamicBootError),
    /// The DTB envelope or owned-copy operation was rejected.
    Dtb(BootDtbImportError),
    /// An internal owned-state invariant was not established.
    InvalidOwnedState,
}

impl From<DynamicBootError> for BootInfoError {
    fn from(error: DynamicBootError) -> Self {
        Self::Dynamic(error)
    }
}

impl From<BootDtbImportError> for BootInfoError {
    fn from(error: BootDtbImportError) -> Self {
        Self::Dtb(error)
    }
}

pub(crate) fn validate_dynamic(
    words: DynamicWords,
    next_address_allowed: impl FnOnce(usize) -> bool,
) -> Result<DynamicBoot, DynamicBootError> {
    if words.magic != DYNAMIC_MAGIC {
        return Err(DynamicBootError::BadMagic);
    }

    let init_hart = match words.version {
        1 => InitHart::FirstArrival,
        2 if words.boot_hart == usize::MAX => InitHart::FirstArrival,
        2 => InitHart::Explicit(words.boot_hart),
        _ => return Err(DynamicBootError::UnsupportedVersion),
    };

    if !next_address_allowed(words.next_address) {
        return Err(DynamicBootError::InvalidNextAddress);
    }
    let next_mode = match words.next_mode {
        0 => NextMode::User,
        1 => NextMode::Supervisor,
        3 => NextMode::Machine,
        _ => return Err(DynamicBootError::InvalidNextMode),
    };

    Ok(DynamicBoot {
        init_hart,
        next_address: words.next_address,
        next_mode,
        options: words.options,
    })
}
