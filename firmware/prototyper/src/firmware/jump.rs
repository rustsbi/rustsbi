use riscv::register::mstatus;

use crate::cfg::JUMP_ADDRESS;

/// The next stage is the configured jump address, entered in S-mode.
pub(crate) fn get_boot_info(_dynamic_info_addr: usize) -> (mstatus::MPP, usize) {
    (mstatus::MPP::Supervisor, JUMP_ADDRESS)
}
