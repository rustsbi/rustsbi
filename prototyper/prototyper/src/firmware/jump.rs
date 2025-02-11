use core::sync::atomic::{AtomicBool, Ordering};
use riscv::register::mstatus;

use super::BootInfo;
use crate::cfg::JUMP_ADDRESS;

/// Determine whether the current hart is boot hart.
///
/// Return true if the current hart is boot hart.
pub fn is_boot_hart(_nonstandard_a2: usize) -> bool {
    static GENESIS: AtomicBool = AtomicBool::new(true);
    GENESIS.swap(false, Ordering::AcqRel)
}

pub fn get_boot_info(_nonstandard_a2: usize) -> BootInfo {
    BootInfo {
        next_address: JUMP_ADDRESS,
        mpp: mstatus::MPP::Supervisor,
    }
}
