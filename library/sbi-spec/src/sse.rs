//! Chapter 17. Supervisor Software Events Extension (EID #0x535345 "SSE").

/// Extension ID for Supervisor Software Events Extension.
#[doc(alias = "SBI_EXT_SSE")]
pub const EID_SSE: usize = crate::eid_from_str("SSE") as _;
pub use fid::*;

/// Declared in Table 90 at §17.17.
mod fid {
    /// Function ID to read software event attributes.
    ///
    /// Declared in §17.7
    #[doc(alias = "SBI_EXT_SSE_READ_ATTR")]
    pub const READ_ATTRS: usize = 0;
    /// Function ID to write software event attributes.
    ///
    /// Declared in §17.8
    #[doc(alias = "SBI_EXT_SSE_WRITE_ATTR")]
    pub const WRITE_ATTRS: usize = 1;
    /// Function ID to register a software event.
    ///
    /// Declared in §17.9.
    #[doc(alias = "SBI_EXT_SSE_REGISTER")]
    pub const REGISTER: usize = 2;
    /// Function ID to unregister a software event.
    ///
    /// Declared in §17.10.
    #[doc(alias = "SBI_EXT_SSE_UNREGISTER")]
    pub const UNREGISTER: usize = 3;
    /// Function ID to enable a software event.
    ///
    /// Declared in §17.11.
    #[doc(alias = "SBI_EXT_SSE_ENABLE")]
    pub const ENABLE: usize = 4;
    /// Function ID to disable a software event.
    ///
    /// Declared in §17.12.
    #[doc(alias = "SBI_EXT_SSE_DISABLE")]
    pub const DISABLE: usize = 5;
    /// Function ID to complete software event handling.
    ///
    /// Declared in §17.13.
    #[doc(alias = "SBI_EXT_SSE_COMPLETE")]
    pub const COMPLETE: usize = 6;
    /// Function ID to inject a software event.
    ///
    /// Declared in §17.14.
    #[doc(alias = "SBI_EXT_SSE_INJECT")]
    pub const INJECT: usize = 7;
    /// Function ID to unmask software events on the calling hart.
    ///
    /// Declared in §17.15.
    #[doc(alias = "SBI_EXT_SSE_HART_UNMASK")]
    pub const HART_UNMASK: usize = 8;
    /// Function ID to mask software events on the calling hart.
    ///
    /// Declared in §17.16.
    #[doc(alias = "SBI_EXT_SSE_HART_MASK")]
    pub const HART_MASK: usize = 9;
}
