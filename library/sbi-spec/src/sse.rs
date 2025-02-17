//! Chapter 17. Supervisor Software Events Extension (EID #0x535345 "SSE").

/// Extension ID for Supervisor Software Events Extension.
#[doc(alias = "sbi_eid_sse")]
pub const EID_SSE: usize = crate::eid_from_str("SSE") as _;
pub use fid::*;

/// Declared in Table 90 at §17.17.
mod fid {
    /// Function ID to read software event attributes.
    ///
    /// Declared in §17.7
    #[doc(alias = "sbi_read_attrs")]
    pub const READ_ATTRS: usize = 0;
    /// Function ID to write software event attributes.
    ///
    /// Declared in §17.8
    #[doc(alias = "sbi_write_attrs")]
    pub const WRITE_ATTRS: usize = 1;
    /// Function ID to register a software event.
    ///
    /// Declared in §17.9.
    #[doc(alias = "sbi_register")]
    pub const REGISTER: usize = 2;
    /// Function ID to unregister a software event.
    ///
    /// Declared in §17.10.
    #[doc(alias = "sbi_unregister")]
    pub const UNREGISTER: usize = 3;
    /// Function ID to enable a software event.
    ///
    /// Declared in §17.11.
    #[doc(alias = "sbi_enable")]
    pub const ENABLE: usize = 4;
    /// Function ID to disable a software event.
    ///
    /// Declared in §17.12.
    #[doc(alias = "sbi_disable")]
    pub const DISABLE: usize = 5;
    /// Function ID to complete software event handling.
    ///
    /// Declared in §17.13.
    #[doc(alias = "sbi_complete")]
    pub const COMPLETE: usize = 6;
    /// Function ID to inject a software event.
    ///
    /// Declared in §17.14.
    #[doc(alias = "sbi_inject")]
    pub const INJECT: usize = 7;
    /// Function ID to unmask software events on the calling hart.
    ///
    /// Declared in §17.15.
    #[doc(alias = "sbi_hart_unmask")]
    pub const HART_UNMASK: usize = 8;
    /// Function ID to mask software events on the calling hart.
    ///
    /// Declared in §17.16.
    #[doc(alias = "sbi_hart_mask")]
    pub const HART_MASK: usize = 9;
}
