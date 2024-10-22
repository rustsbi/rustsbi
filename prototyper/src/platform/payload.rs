use super::BootInfo;
use core::arch::asm;

use riscv::register::mstatus;

pub fn get_boot_info(_opaque: usize, _nonstandard_a2: usize) -> BootInfo {
    BootInfo {
        fdt_address: get_fdt_address(),
        next_address: get_image_address(),
        mpp: mstatus::MPP::Supervisor,
        is_boot_hart: true,
    }
}

#[naked]
#[link_section = ".fw_fdt"]
pub unsafe extern "C" fn raw_fdt() {
    asm!(
        concat!(".incbin \"", env!("PROTOTYPER_FDT"), "\""),
        options(noreturn)
    );
}

#[naked]
#[link_section = ".payload"]
pub unsafe extern "C" fn payload_image() {
    asm!(
        concat!(".incbin \"", env!("PROTOTYPER_IMAGE"), "\""),
        options(noreturn)
    );
}

#[inline]
fn get_fdt_address() -> usize {
    raw_fdt as usize
}

#[inline]
fn get_image_address() -> usize {
    payload_image as usize
}
