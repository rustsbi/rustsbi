use riscv::register::mstatus;

use super::BootInfo;
use crate::cfg::JUMP_ADDRESS;

pub fn get_boot_info(_nonstandard_a2: usize) -> BootInfo {
    BootInfo {
        next_address: JUMP_ADDRESS,
        mpp: mstatus::MPP::Supervisor,
    }
}
