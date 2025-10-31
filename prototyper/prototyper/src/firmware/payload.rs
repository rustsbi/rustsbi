use core::arch::naked_asm;
use riscv::register::mstatus;

use super::BootInfo;

pub fn get_boot_info(_nonstandard_a2: usize) -> BootInfo {
    BootInfo {
        next_address: get_image_address(),
        mpp: mstatus::MPP::Supervisor,
    }
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
