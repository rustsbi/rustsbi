use core::arch::naked_asm;
use riscv::register::mstatus;

/// The next stage is the embedded payload image, entered in S-mode.
pub(crate) fn get_boot_info(_dynamic_info_addr: usize) -> (mstatus::MPP, usize) {
    (mstatus::MPP::Supervisor, get_image_address())
}

#[unsafe(naked)]
#[unsafe(link_section = ".payload")]
pub extern "C" fn payload_image() {
    naked_asm!(concat!(".incbin \"", env!("PROTOTYPER_PAYLOAD_PATH"), "\""),)
}

#[inline]
fn get_image_address() -> usize {
    payload_image as usize
}
