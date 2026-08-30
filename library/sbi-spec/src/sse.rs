//! Chapter 17. Supervisor Software Events Extension (EID #0x535345 "SSE").

/// Extension ID for Supervisor Software Events Extension.
#[doc(alias = "SBI_EXT_SSE")]
pub const EID_SSE: usize = crate::eid_from_str("SSE") as _;
pub use fid::*;

/// Standard supervisor software event identifiers.
pub mod event_id {
    /// Local high-priority RAS event.
    pub const LOCAL_HIGH_PRIORITY_RAS: u32 = 0x0000_0000;
    /// Local double-trap event.
    pub const LOCAL_DOUBLE_TRAP: u32 = 0x0000_0001;
    /// Global high-priority RAS event.
    pub const GLOBAL_HIGH_PRIORITY_RAS: u32 = 0x0000_8000;
    /// Local PMU overflow event.
    pub const LOCAL_PMU_OVERFLOW: u32 = 0x0001_0000;
    /// Local low-priority RAS event.
    pub const LOCAL_LOW_PRIORITY_RAS: u32 = 0x0010_0000;
    /// Global low-priority RAS event.
    pub const GLOBAL_LOW_PRIORITY_RAS: u32 = 0x0010_8000;
    /// Software-injected local event.
    pub const SOFTWARE_INJECTED_LOCAL: u32 = 0xffff_0000;
    /// Software-injected global event.
    pub const SOFTWARE_INJECTED_GLOBAL: u32 = 0xffff_8000;
}

/// Supervisor software event attribute identifiers.
pub mod attr_id {
    /// Event state, pending state, and injection capability.
    pub const STATUS: u32 = 0;
    /// Event priority.
    pub const PRIORITY: u32 = 1;
    /// Additional event configuration.
    pub const CONFIG: u32 = 2;
    /// Preferred hart for a global event.
    pub const PREFERRED_HART: u32 = 3;
    /// Supervisor event handler entry address.
    pub const ENTRY_PC: u32 = 4;
    /// Argument passed to the supervisor event handler.
    pub const ENTRY_ARG: u32 = 5;
    /// Saved supervisor exception program counter.
    pub const INTERRUPTED_SEPC: u32 = 6;
    /// Saved supervisor and virtualization flags.
    pub const INTERRUPTED_FLAGS: u32 = 7;
    /// Saved `a6` register.
    pub const INTERRUPTED_A6: u32 = 8;
    /// Saved `a7` register.
    pub const INTERRUPTED_A7: u32 = 9;
}

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
