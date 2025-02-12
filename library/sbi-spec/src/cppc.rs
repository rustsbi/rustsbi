//! Chapter 14. CPPC Extension (EID #0x43505043 "CPPC").

/// Extension ID for CPPC Extension.
pub const EID_CPPC: usize = crate::eid_from_str("CPPC") as _;
pub use fid::*;

/// Declared in §14.
mod fid {
    /// Function ID to probe a CPPC register.
    ///
    /// Declared in §14.1.
    pub const PROBE: usize = 0;
    /// Function ID to read CPPC register bits.
    ///
    /// Declared in §14.2.
    pub const READ: usize = 1;
    /// Function ID to read high bits of a CPPC register.
    ///
    /// Declared in §14.3.
    pub const READ_HI: usize = 2;
    /// Function ID to write to a CPPC register.
    ///
    /// Declared in §14.4.
    pub const WRITE: usize = 3;
}
