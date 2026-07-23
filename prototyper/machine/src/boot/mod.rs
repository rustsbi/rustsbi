//! Owned boot inputs and their raw import boundary.

// Production ownership is entered only by the RISC-V raw entry.

pub(crate) mod device_tree;
mod dtb;
pub(crate) mod import;
mod prepare;
mod protocol;
mod resources;
mod to_next;

pub use dtb::BootDtb;
pub(crate) use dtb::BootDtbImportError;
#[cfg(test)]
use dtb::{
    BootDtbStorage, DTB_HEADER_SIZE, DTB_MAGIC, copy_from_entry, encode_handoff_dtb,
    validate_envelope,
};
#[cfg(feature = "mtest")]
pub(crate) use prepare::prepare_runtime;
pub use protocol::NextStage;
pub(crate) use protocol::{BootInfoError, NextMode};
pub use resources::BootInfo;
pub(crate) use resources::MachineRangeError;
pub(crate) use to_next::{enter, enter_warm_hart};

#[cfg(test)]
mod tests;
