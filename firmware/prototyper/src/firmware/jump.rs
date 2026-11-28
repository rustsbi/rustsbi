use riscv_csr_spec::standard::isa::values::status::Privilege;

use crate::cfg::JUMP_ADDRESS;

/// The next stage is the configured jump address, entered in S-mode.
pub(crate) fn decode_next_stage(_dynamic_info_address: usize) -> (Privilege, usize) {
    (Privilege::Supervisor, JUMP_ADDRESS)
}
