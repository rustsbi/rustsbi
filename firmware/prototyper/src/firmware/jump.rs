use riscv::register::mstatus;

use crate::cfg::JUMP_ADDRESS;

/// The next stage is the configured jump address, entered in S-mode.
pub(crate) fn decode_next_stage(_dynamic_info_address: usize) -> (mstatus::MPP, usize) {
    (mstatus::MPP::Supervisor, JUMP_ADDRESS)
}
