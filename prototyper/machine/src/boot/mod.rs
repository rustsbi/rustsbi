//! Owned boot inputs and their raw import boundary.

// Production ownership is entered only by the target-specific raw entry. Host
// builds exercise the pure validators and model, so some target-only items are
// intentionally unreachable in the library half of an all-targets build.

pub(crate) mod device_tree;
mod dtb;
#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
mod handoff;
#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
pub(crate) mod import;
#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
mod prepare;
mod protocol;
mod resources;

pub use dtb::BootDtb;
pub(crate) use dtb::BootDtbImportError;
#[cfg(test)]
use dtb::{
    BootDtbStorage, DTB_HEADER_SIZE, DTB_MAGIC, copy_from_entry, encode_handoff_dtb,
    validate_envelope,
};
#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
pub(crate) use handoff::{enter, enter_warm_hart};
#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
pub use prepare::enter_next_stage;
#[cfg(all(
    feature = "mtest",
    any(target_arch = "riscv32", target_arch = "riscv64")
))]
pub(crate) use prepare::prepare_runtime;
pub use protocol::NextStage;
pub(crate) use protocol::{BootInfoError, NextMode};
#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
#[cfg(not(any(feature = "jump", feature = "payload")))]
pub(crate) use protocol::{DYNAMIC_MAGIC, DynamicWords};
pub use resources::BootInfo;
pub(crate) use resources::MachineRangeError;

#[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
mod unsupported;
#[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
pub use unsupported::enter_next_stage;

#[cfg(test)]
mod tests;
