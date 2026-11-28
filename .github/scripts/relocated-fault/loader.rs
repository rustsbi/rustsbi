//! Boots the firmware at a load offset and faults before Runtime installs
//! its own trap vector, so the early vector must be position independent.
#![no_std]
#![no_main]

use core::arch::global_asm;

// The invalid `a1` faults on the first device-tree read, which happens after
// the early vector is installed and before Runtime replaces it.
global_asm!(
    r#"
    .section .text.entry, "ax"
    .globl _start
_start:
    lla  a2, dynamic_info
    li   a1, 0xdeadb000
    li   t0, 0x80200000
    jr   t0
    .balign 8
dynamic_info:
"#
);
#[cfg(target_pointer_width = "64")]
global_asm!(".dword 0x4942534f, 2, 0x84000000, 3, 0, 0");
#[cfg(target_pointer_width = "32")]
global_asm!(".word 0x4942534f, 2, 0x84000000, 3, 0, 0");

#[panic_handler]
fn panic(_: &core::panic::PanicInfo<'_>) -> ! {
    loop {
        core::hint::spin_loop();
    }
}
