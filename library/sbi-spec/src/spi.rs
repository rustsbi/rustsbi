//! Chapter 7. IPI Extension (EID #0x735049 "sPI: s-mode IPI").

/// Extension ID for Inter-processor Interrupt extension.
#[doc(alias = "SBI_EXT_IPI")]
pub const EID_SPI: usize = crate::eid_from_str("sPI") as _;
pub use fid::*;

/// Declared in §7.2.
mod fid {
    /// Function ID to send an inter-processor interrupt to all harts defined in hart mask.
    ///
    /// Declared in §7.1.
    #[doc(alias = "SBI_EXT_IPI_SEND_IPI")]
    pub const SEND_IPI: usize = 0;
}
