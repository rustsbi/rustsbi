#![no_std]
#![no_main]

// TODO: RustSBI EFI module
#[no_mangle]
extern "C" fn rust_main(_hart_id: usize, _opaque: usize) {
    // TODO
}

#[no_mangle]
extern "C" fn rust_sbi_exit() -> ! {
    sbi_rt::system_reset(sbi_rt::Shutdown, sbi_rt::NoReason);
    loop {
        core::hint::spin_loop();
    }
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    // TODO panic handler
    loop {
        core::hint::spin_loop();
    }
}

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
        // 2. Initialize programming langauge runtime
        // only initialize if it is boot hart (hart ID 0)
        "   bnez    a0, 3f",
        // clear bss segment
        "   la      t0, sbss
            la      t1, ebss
        2:  bgeu    t0, t1, 2f
            sd      zero, 0(t0)
            addi    t0, t0, 8
            j       2b",
        // prepare data segment
        "   la      t3, sidata
            la      t4, sdata
            la      t5, edata
        2:  bgeu    t4, t5, 2f
            ld      t6, 0(t3)
            sd      t6, 0(t4)
            addi    t3, t3, 8
            addi    t4, t4, 8
            j       2b",
        "3:",
        // 3. Prepare stack for each hart
        "   la      sp, {stack}
            li      t1, {per_hart_stack_size}
            addi    t2, a0, 1
        2:  add     sp, sp, t1
            addi    t2, t2, -1
            bnez    t2, 2b",
        // 4. Start main function
        "   call    {main}",
        "   call    {exit}",
        stack = sym STACK,
        per_hart_stack_size = const LEN_STACK_PER_HART,
        main = sym rust_main,
        exit = sym rust_sbi_exit
    )
}
