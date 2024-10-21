use core::arch::asm;

#[naked]
#[link_section = ".fw_fdt"]
pub unsafe extern "C" fn raw_fdt() {
    asm!(
        concat!(".incbin \"", env!("PROTOTYPER_FDT"), "\""),
        options(noreturn)
    );
}

#[naked]
#[link_section = ".fw_payload"]
pub unsafe extern "C" fn fw_payload_image() {
    asm!(
        concat!(".incbin \"", env!("PROTOTYPER_IMAGE"), "\""),
        options(noreturn)
    );
}

#[inline]
pub fn get_fdt_address() -> usize {
    raw_fdt as usize
}

#[inline]
pub fn get_image_address() -> usize {
    fw_payload_image as usize
}
