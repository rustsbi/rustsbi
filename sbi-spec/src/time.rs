//! Chapter 6. Timer Extension (EID #0x54494D45 "TIME").

/// Extension ID for Timer extension.
pub const EID_TIME: usize = crate::eid_from_str("TIME") as _;
pub use fid::*;

/// Declared in §6.2.
mod fid {
    /// Function ID to program the clock for next event after an absolute time.
    ///
    /// Declared in §6.1.
    pub const SET_TIMER: usize = 0;
}
