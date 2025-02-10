//! Chapter 16. Steal-time Accounting Extension (EID #0x535441 "STA").

/// Extension ID for Steal-time Accounting Extension.
pub const EID_STA: usize = crate::eid_from_str("STA") as _;
pub use fid::*;

/// Declared in §16.2.
mod fid {
    /// Function ID to set the shared memory physical base address for steal-time accounting of the calling virtual hart and enable the SBI implementation’s steal-time information reporting.
    ///
    /// Declared in §16.1.
    pub const SET_SHMEM: usize = 0;
}
