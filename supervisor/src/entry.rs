const LEN_STACK_PER_HART: usize = 16 * 1024;
pub const NUM_HART_MAX: usize = 8;
#[link_section = ".bss.uninit"]
static mut STACK: [u8; NUM_HART_MAX * LEN_STACK_PER_HART] = [0; NUM_HART_MAX * LEN_STACK_PER_HART];

// If booted with RISC-V SBI, a0 must include hart ID, while a1 must be an opaque register
#[naked_function::naked]
#[link_section = ".text.entry"]
#[export_name = "_start"]
unsafe extern "C" fn start() -> ! {
    asm!(
        // 1. Turn off interrupt
        "   csrw    sie, zero",
        // 2. Initialize programming language runtime
        // only initialize if it is boot hart (hart ID 0)
        "   bnez    a0, 4f",
        // clear bss segment
        "   la      t0, sbss
            la      t1, ebss
        1:  bgeu    t0, t1, 2f
            sd      zero, 0(t0)
            addi    t0, t0, 8
            j       1b",
        "2:",
        // prepare data segment
        "   la      t3, sidata
            la      t4, sdata
            la      t5, edata
        3:  bgeu    t4, t5, 4f
            ld      t6, 0(t3)
            sd      t6, 0(t4)
            addi    t3, t3, 8
            addi    t4, t4, 8
            j       3b",
        "4:",
        "   la      sp, {stack}
            li      t0, {per_hart_stack_size}
            addi    t1, a0, 1
        5:  add     sp, sp, t0
            addi    t1, t1, -1
            bnez    t1, 5b",
        // 4. Start main function
        "   call    {main}",
        "   call    {exit}",
        stack = sym STACK,
        per_hart_stack_size = const LEN_STACK_PER_HART,
        main = sym crate::rust_main,
        exit = sym rust_sbi_exit
    )
}

#[no_mangle]
extern "C" fn rust_sbi_exit() -> ! {
    sbi_rt::system_reset(sbi_rt::Shutdown, sbi_rt::NoReason);
    loop {
        core::hint::spin_loop();
    }
}
