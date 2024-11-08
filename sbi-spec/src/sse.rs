//! Chapter 17. Supervisor Software Events Extension (EID #0x535345 "SSE").

/// Extension ID for Supervisor Software Events Extension.
pub const EID_SSE: usize = crate::eid_from_str("SSE") as _;
pub use fid::*;

/// Declared in Table 90 at §17.17.
mod fid {
    /// Function ID to read software event attributes.
    ///
    /// Declared in §17.7
    pub const READ_ATTRS: usize = 0;
    /// Function ID to write software event attributes.
    ///
    /// Declared in §17.8
    pub const WRITE_ATTRS: usize = 1;
    /// Function ID to register a software event.
    ///
    /// Declared in §17.9.
    pub const REGISTER: usize = 2;
    /// Function ID to unregister a software event.
    ///
    /// Declared in §17.10.
    pub const UNREGISTER: usize = 3;
    /// Function ID to enable a software event.
    ///
    /// Declared in §17.11.
    pub const ENABLE: usize = 4;
    /// Function ID to disable a software event.
    ///
    /// Declared in §17.12.
    pub const DISABLE: usize = 5;
    /// Function ID to complete software event handling.
    ///
    /// Declared in §17.13.
    pub const COMPLETE: usize = 6;
    /// Function ID to inject a software event.
    ///
    /// Declared in §17.14.
    pub const INJECT: usize = 7;
    /// Function ID to unmask software events on the calling hart.
    ///
    /// Declared in §17.15.
    pub const HART_UNMASK: usize = 8;
    /// Function ID to mask software events on the calling hart.
    ///
    /// Declared in §17.16.
    pub const HART_MASK: usize = 9;
}
