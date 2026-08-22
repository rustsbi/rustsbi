//! The typed tree of run-policy defaults and emulator parameters.
//!
//! Leaf module: owns nouns, imports nothing from the pipeline. Code-level
//! constants today; the serde derives prewire the `run` command's future
//! user-editable TOML.

use serde::{Deserialize, Serialize};

/// Which action section of [`Scheme`] a command reads.
/// Owned here (not in `kernels`) so this module stays a leaf.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum Action {
    Test,
    Bench,
}

/// Per-action run defaults.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ActionDefaults {
    /// Harts QEMU boots the kernel with.
    pub(crate) smp: usize,
    /// Timeout of one QEMU attempt, in seconds.
    pub(crate) timeout_secs: u64,
    /// Total QEMU attempts; retries happen only after a timeout.
    pub(crate) attempts: usize,
}

/// QEMU invocation parameters shared by every action.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct QemuParams {
    /// Machine model (`-machine`).
    pub(crate) machine: String,
    /// Guest memory in MiB (`-m`).
    pub(crate) memory_mb: u64,
}

/// Single source of run-policy defaults.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Scheme {
    /// Defaults for `cargo prototyper test`.
    pub(crate) test: ActionDefaults,
    /// Defaults for `cargo prototyper bench`.
    pub(crate) bench: ActionDefaults,
    /// QEMU parameters for both actions.
    pub(crate) qemu: QemuParams,
}

impl Scheme {
    /// The run defaults for one action section. The only lookup API;
    /// callers never index `.test`/`.bench` directly.
    pub(crate) fn action(&self, action: Action) -> &ActionDefaults {
        match action {
            Action::Test => &self.test,
            Action::Bench => &self.bench,
        }
    }
}

// Hand-written: the real values, not zeros.
impl Default for Scheme {
    fn default() -> Self {
        Scheme {
            test: ActionDefaults {
                smp: 1,
                timeout_secs: 60,
                attempts: 2,
            },
            bench: ActionDefaults {
                smp: 4,
                timeout_secs: 90,
                attempts: 4,
            },
            qemu: QemuParams {
                machine: "virt".to_string(),
                memory_mb: 256,
            },
        }
    }
}
