//! Repeated machine interrupts must preserve the payload's Runtime trap stack.
#![no_std]
#![no_main]

use core::arch::global_asm;

// QEMU virt, hart 0. A cleared MSIP confirms that Runtime handled each real
// machine software interrupt. Both returns must republish the clean trap stack.
global_asm!(
    ".section .text.entry, \"ax\"",
    ".globl _start",
    "_start:",
    "li sp, 0x85000000",
    "mv s3, sp",
    "csrr s0, mscratch",
    "beqz s0, 3f",
    "li s1, 2",
    "2:",
    "li t0, 0x2000000",
    "li t1, 1",
    "sw t1, 0(t0)",
    "fence iorw, iorw",
    "4: lw t1, 0(t0)",
    "bnez t1, 4b",
    "csrr t1, mscratch",
    "bne t1, s0, 3f",
    "bne sp, s3, 3f",
    "addi s1, s1, -1",
    "bnez s1, 2b",
    "lla a0, pass",
    "li s2, 0x5555",
    "j 5f",
    "3:",
    "lla a0, fail",
    "li s2, 0x13333",
    "5:",
    "li t0, 0x10000000",
    "6: lbu t1, 0(a0)",
    "beqz t1, 7f",
    "sb t1, 0(t0)",
    "addi a0, a0, 1",
    "j 6b",
    "7:",
    "li t0, 0x100000",
    "sw s2, 0(t0)",
    "8: wfi",
    "j 8b",
    ".section .rodata",
    "pass: .asciz \"[machine-stack] two interrupt returns pass\\n\"",
    "fail: .asciz \"[machine-stack] Runtime stack lost\\n\"",
);

#[panic_handler]
fn panic(_: &core::panic::PanicInfo<'_>) -> ! {
    loop {
        core::hint::spin_loop();
    }
}
