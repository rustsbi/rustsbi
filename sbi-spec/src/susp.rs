//! Chapter 13. System Suspend Extension (EID #0x53555350 "SUSP").

/// Extension ID for System Suspend Extension.
pub const EID_SUSP: usize = crate::eid_from_str("SUSP") as _;
pub use fid::*;

/// Declared in §13.
mod fid {
    /// Function ID to suspend under system-level sleep states.
    ///
    /// Declared in §13.1.
    pub const SUSPEND: usize = 0;
}
