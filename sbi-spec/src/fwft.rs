//! Chapter 18. Firmware Features Extension (EID #0x46574654 "FWFT").

/// Extension ID for Firmware Features Extension.
pub const EID_FWFT: usize = crate::eid_from_str("FWFT") as _;
pub use fid::*;

/// Declared in §18.3.
mod fid{
    /// Set the firmware function of the request based on Value and Flags parameters.
    ///
    /// Declared in §18.1.
    pub const SET: usize = 0;
    /// Return to the firmware function configuration value.
    ///
    /// Declared in §18.2.
    pub const GET: usize = 1;
}
