use riscv::register::mstatus;

/// The next stage is the embedded payload image, entered in S-mode.
pub(crate) fn decode_next_stage(_dynamic_info_address: usize) -> (mstatus::MPP, usize) {
    (mstatus::MPP::Supervisor, payload_address())
}

#[inline]
fn payload_address() -> usize {
    runtime::boot::embedded_payload()
        .expect("BUG: payload firmware has no embedded payload")
        .start()
        .as_usize()
}
