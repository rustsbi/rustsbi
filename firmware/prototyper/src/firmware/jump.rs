use csr::machine::TargetPrivilege;

use crate::cfg::JUMP_ADDRESS;

/// The next stage is the configured jump address, entered in S-mode.
pub(crate) fn decode_next_stage(_dynamic_info_address: usize) -> (TargetPrivilege, usize) {
    (TargetPrivilege::Supervisor, JUMP_ADDRESS)
}
