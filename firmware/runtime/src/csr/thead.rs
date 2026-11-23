//! T-Head CSR operations used during per-hart startup.

use csr::{
    machine::{Identifier, VendorId},
    vendor::thead::machine::ExtendedStatus,
};

/// Disables non-standard page memory attributes on a T-Head hart.
///
/// Other vendors need no change.
///
/// # Safety
///
/// Run during M-mode startup after Runtime trap entry is installed, with
/// `mscratch` zero and `MPRV` clear. No lower-mode address space may be active.
pub unsafe fn disable_maee() {
    const THEAD_VENDOR_ID: Identifier = Identifier::from_bits(0x5b7);
    if VendorId::read() != THEAD_VENDOR_ID {
        return;
    }
    // Runtime entry recovers an unavailable mxstatus before any MAEE update.
    if super::csrr!(ExtendedStatus).is_ok() {
        // SAFETY: The caller owns translation setup and the CSR probe succeeded.
        unsafe { ExtendedStatus::clear_maee() };
    }
}
