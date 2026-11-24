//! QEMU bootstrap for a dynamic handoff to an M-mode test payload.
#![no_std]
#![no_main]

use core::arch::global_asm;

// QEMU's reset vector supplies the hart ID and FDT in a0/a1. Preserve them
// while replacing a2 with a DynamicInfo record whose next_mode is Machine.
global_asm!(
    ".section .text.entry, \"ax\"",
    ".globl _start",
    "_start:",
    "lla a2, dynamic_info",
    "li t0, 0x80400000",
    "jr t0",
    ".balign 8",
    "dynamic_info:",
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
