#![feature(naked_functions, asm_const)]
#![no_std]
#![no_main]

use panic_halt as _;

extern "C" fn main(hart_id: usize, opaque: usize, a2: usize) -> usize {
    let _ = (hart_id, opaque, a2);
    0 // TODO
}

const LEN_STACK_PER_HART: usize = 16 * 1024;
pub(crate) const NUM_HART_MAX: usize = 8;
const LEN_STACK: usize = LEN_STACK_PER_HART * NUM_HART_MAX;

// TODO contribute `Stack` struct into the crate `riscv`
#[repr(C, align(128))]
struct Stack<const N: usize>([u8; N]);

#[link_section = ".bss.uninit"]
static STACK: Stack<LEN_STACK> = Stack([0; LEN_STACK]);

#[naked]
#[link_section = ".text.entry"]
#[export_name = "_start"]
unsafe extern "C" fn start() -> ! {
    core::arch::asm!(
        // 1. Turn off interrupt
        "   csrw    mie, zero",
        // 2. Initialize programming langauge runtime
        // only clear bss if hartid is zero
        "   csrr    t0, mhartid",
        "   bnez    t0, 2f",
        // clear bss segment
        "   la      t0, sbss
            la      t1, ebss
        1:  bgeu    t0, t1, 2f
            sd      zero, 0(t0)
            addi    t0, t0, 8
            j       1b",
        // prepare data segment
        "   la      t3, sidata
            la      t4, sdata
            la      t5, edata
        1:  bgeu    t4, t5, 2f
            ld      t6, 0(t3)
            sd      t6, 0(t4)
            addi    t3, t3, 8
            addi    t4, t4, 8
            j       1b",
        "2: ",
        // 3. Prepare stack for each hart
        "   la      sp, {stack}",
        "   li      t0, {stack_size_per_hart}",
        "   csrr    t1, mhartid",
        "   addi    t1, t1, 1",
        "1: ",
        "   add     sp, sp, t0",
        "   addi    t1, t1, -1",
        "   bnez    t1, 1b",
        // 4. Run Rust main function
        "   j       {main}",
        // 5. Jump to following boot sequences
        "   jr      a0", // TODO
        stack_size_per_hart = const LEN_STACK_PER_HART,
        stack = sym STACK,
        main = sym main,
        options(noreturn)
    )
}
