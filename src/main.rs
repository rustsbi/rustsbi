#![feature(naked_functions, asm_const)]
#![no_std]
#![no_main]

mod dynamic;

use panic_halt as _;
use riscv::register::mstatus;

extern "C" fn main(hart_id: usize, opaque: usize, nonstandard_a2: usize) -> usize {
    let _ = (hart_id, opaque);

    let info = dynamic::read_paddr(nonstandard_a2).unwrap_or_else(fail_no_dynamic_info_available);

    let (mpp, next_addr) = dynamic::mpp_next_addr(&info).unwrap_or_else(fail_invalid_dynamic_info);

    unsafe { mstatus::set_mpp(mpp) };
    next_addr
}

#[cold]
fn fail_invalid_dynamic_info(_err: dynamic::DynamicError) -> (mstatus::MPP, usize) {
    // TODO dynamic information contains invalid privilege mode or next address
    loop {
        core::hint::spin_loop()
    }
}

#[cold]
fn fail_no_dynamic_info_available(_err: ()) -> dynamic::DynamicInfo {
    // TODO no dynamic information available
    loop {
        core::hint::spin_loop()
    }
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
        "   call    {main}",
        // 5. Jump to following boot sequences
        "   csrw    mepc, a0",
        "   mret",
        stack_size_per_hart = const LEN_STACK_PER_HART,
        stack = sym STACK,
        main = sym main,
        options(noreturn)
    )
}
