//! Chapter 12. Debug Console Extension (EID #0x4442434E "DBCN").

/// Extension ID for Debug Console Extension.
#[doc(alias = "SBI_EXT_DBCN")]
pub const EID_DBCN: usize = crate::eid_from_str("DBCN") as _;
pub use fid::*;

/// Declared in §12.
mod fid {
    /// Function ID to write bytes to the debug console from input memory.
    ///
    /// Declared in §12.1.
    #[doc(alias = "SBI_EXT_DBCN_CONSOLE_WRITE")]
    pub const CONSOLE_WRITE: usize = 0;
    /// Function ID to read bytes from the debug console into an output memory.
    ///
    /// Declared in §12.2.
    #[doc(alias = "SBI_EXT_DBCN_CONSOLE_READ")]
    pub const CONSOLE_READ: usize = 1;
    /// Function ID to write a single byte to the debug console.
    ///
    /// Declared in §12.3.
    #[doc(alias = "SBI_EXT_DBCN_CONSOLE_WRITE_BYTE")]
    pub const CONSOLE_WRITE_BYTE: usize = 2;
}
