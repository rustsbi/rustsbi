use csr::machine::TargetPrivilege;

/// The next stage is the embedded payload image, entered in S-mode.
pub(crate) fn decode_next_stage(_dynamic_info_address: usize) -> (TargetPrivilege, usize) {
    (TargetPrivilege::Supervisor, payload_address())
}

#[inline]
fn payload_address() -> usize {
    payload_image.address().as_usize()
}

include!(concat!(env!("OUT_DIR"), "/generated_alignment.rs"));
include!(concat!(env!("OUT_DIR"), "/generated_payload.rs"));
#[cfg(feature = "fdt")]
include!(concat!(env!("OUT_DIR"), "/generated_fdt.rs"));
