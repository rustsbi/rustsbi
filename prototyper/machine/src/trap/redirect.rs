//! Supervisor trap redirection and hypervisor metadata commit.

use super::frame::HypervisorTrap;

mod arch;

pub(super) use arch::{read_supervisor_vector, write_supervisor_trap};

pub(super) fn hypervisor_status(original: usize, metadata: HypervisorTrap) -> (usize, usize) {
    const GVA: usize = 1 << 6;
    const SPV: usize = 1 << 7;
    const SPVP: usize = 1 << 8;

    let mut mask = GVA | SPV;
    let mut desired = original & !mask;
    if metadata.guest_address {
        desired |= GVA;
    }
    if metadata.virtualized {
        mask |= SPVP;
        desired &= !SPVP;
        desired |= SPV;
        if metadata.previous_supervisor {
            desired |= SPVP;
        }
    }
    (desired, mask)
}
