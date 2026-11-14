//! HFENCE dispatch policy: target preflight and GVMA address encoding.
//!
//! The helpers only depend on `alloc`, without Runtime, CSR or assembly
//! dependencies.

extern crate alloc;

use alloc::vec::Vec;

/// Collects a snapshot of the fence targets and rejects the whole call when
/// any target lacks the hypervisor extension.
///
/// Returns `Some` with the collected targets in iteration order when every
/// target supports the hypervisor extension, and `None` otherwise. An empty
/// target set succeeds with an empty snapshot. Collecting the snapshot up
/// front keeps the validated set and the dispatched set identical even when
/// hart availability changes between the two phases.
pub(crate) fn preflight_hfence_targets<T>(
    targets: impl IntoIterator<Item = T>,
    supports_h: impl FnMut(&T) -> bool,
) -> Option<Vec<T>> {
    let snapshot: Vec<T> = targets.into_iter().collect();
    if snapshot.iter().all(supports_h) {
        Some(snapshot)
    } else {
        None
    }
}

/// Encodes a byte guest physical address as the `rs1` operand of
/// `hfence.gvma`, which takes the guest physical page number (`GPA >> 2`).
pub(crate) const fn encode_gvma_address(guest_physical_address: usize) -> usize {
    guest_physical_address >> 2
}
