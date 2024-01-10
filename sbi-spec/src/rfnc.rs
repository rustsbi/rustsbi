//! Chapter 8. RFENCE Extension (EID #0x52464E43 "RFNC").

/// Extension ID for Remote Fence extension.
pub const EID_RFNC: usize = crate::eid_from_str("RFNC") as _;
pub use fid::*;

/// Declared in §8.8.
mod fid {
    /// Function ID to `FENCE.I` instruction on remote harts.
    ///
    /// Declared in §8.1.
    pub const REMOTE_FENCE_I: usize = 0;
    /// Function ID to `SFENCE.VMA` for all address spaces on remote harts.
    ///
    /// Declared in §8.2.
    pub const REMOTE_SFENCE_VMA: usize = 1;
    /// Function ID to address space based `SFENCE.VMA` on remote harts.
    ///
    /// Declared in §8.3.
    pub const REMOTE_SFENCE_VMA_ASID: usize = 2;
    /// Function ID to virtual machine id based `HFENCE.GVMA` on remote harts.
    ///
    /// Declared in §8.4.
    pub const REMOTE_HFENCE_GVMA_VMID: usize = 3;
    /// Function ID to `HFENCE.GVMA` for all virtual machines on remote harts.
    ///
    /// Declared in §8.5.
    pub const REMOTE_HFENCE_GVMA: usize = 4;
    /// Function ID to address space based `HFENCE.VVMA` for current virtual machine on remote harts.
    ///
    /// Declared in §8.6.
    pub const REMOTE_HFENCE_VVMA_ASID: usize = 5;
    /// Function ID to `HFENCE.VVMA` for all address spaces in current virtual machine on remote harts.
    ///
    /// Declared in §8.7.
    pub const REMOTE_HFENCE_VVMA: usize = 6;
}
