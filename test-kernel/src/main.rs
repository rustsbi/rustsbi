// A test kernel to test RustSBI function on all platforms
#![feature(naked_functions, asm)]
#![no_std]
#![no_main]

#[macro_use]
mod console;
mod sbi;

use riscv::register::{sepc, stvec::{self, TrapMode}, scause::{self, Trap, Exception}};

pub extern "C" fn rust_main(hartid: usize, dtb_pa: usize) -> ! {
    println!("<< Test-kernel: Hart id = {}, DTB physical address = {:#x}", hartid, dtb_pa);
    test_base_extension();
    test_sbi_ins_emulation();
    unsafe { stvec::write(start_trap as usize, TrapMode::Direct) };
    println!(">> Test-kernel: Trigger illegal exception");
    unsafe { asm!("csrw mcycle, x0") }; // mcycle cannot be written, this is always a 4-byte illegal instruction
    println!("<< Test-kernel: SBI test SUCCESS, shutdown");
    sbi::shutdown()
}

fn test_base_extension() {
    println!(">> Test-kernel: Testing base extension");
    let base_version = sbi::probe_extension(sbi::EXTENSION_BASE);
    if base_version == 0 {
        println!("!! Test-kernel: no base extension probed; SBI call returned value '0'");
        println!("!! Test-kernel: This SBI implementation may only have legacy extension implemented");
        println!("!! Test-kernel: SBI test FAILED due to no base extension found");
        sbi::shutdown()
    }
    println!("<< Test-kernel: Base extension version: {:x}", base_version);
    println!("<< Test-kernel: SBI specification version: {:x}", sbi::get_spec_version());
    println!("<< Test-kernel: SBI implementation Id: {:x}", sbi::get_sbi_impl_id());
    println!("<< Test-kernel: SBI implementation version: {:x}", sbi::get_sbi_impl_version());
    println!("<< Test-kernel: Device mvendorid: {:x}", sbi::get_mvendorid());
    println!("<< Test-kernel: Device marchid: {:x}", sbi::get_marchid());
    println!("<< Test-kernel: Device mimpid: {:x}", sbi::get_mimpid());
}

fn test_sbi_ins_emulation() {
    println!(">> Test-kernel: Testing SBI instruction emulation");
    let time = riscv::register::time::read64();
    println!("<< Test-kernel: Current time: {:x}", time);
}

pub extern "C" fn rust_trap_exception() {
    let cause = scause::read().cause();
    println!("<< Test-kernel: Value of scause: {:?}", cause);
    if cause != Trap::Exception(Exception::IllegalInstruction) {
        println!("!! Test-kernel: Wrong cause associated to illegal instruction");
        sbi::shutdown()
    }
    println!("<< Test-kernel: Illegal exception delegate success");
    sepc::write(sepc::read().wrapping_add(4));
}

use core::panic::PanicInfo;

#[cfg_attr(not(test), panic_handler)]
#[allow(unused)]
fn panic(info: &PanicInfo) -> ! {
    println!("!! Test-kernel: {}", info);
    println!("!! Test-kernel: SBI test FAILED due to panic");
    sbi::shutdown()
}

const BOOT_STACK_SIZE: usize = 4096 * 4 * 8;

static mut BOOT_STACK: [u8; BOOT_STACK_SIZE] = [0; BOOT_STACK_SIZE];

#[naked]
#[link_section = ".text.entry"] 
#[export_name = "_start"]
unsafe extern "C" fn entry() -> ! {
    asm!("
    # 1. set sp
    # sp = bootstack + (hartid + 1) * 0x10000
    add     t0, a0, 1
    slli    t0, t0, 14
1:  auipc   sp, %pcrel_hi({boot_stack})
    addi    sp, sp, %pcrel_lo(1b)
    add     sp, sp, t0

    # 2. jump to rust_main (absolute address)
1:  auipc   t0, %pcrel_hi({rust_main})
    addi    t0, t0, %pcrel_lo(1b)
    jr      t0
    ", 
    boot_stack = sym BOOT_STACK, 
    rust_main = sym rust_main,
    options(noreturn))
}


#[cfg(target_pointer_width = "128")]
macro_rules! define_store_load {
    () => {
        ".altmacro
        .macro STORE reg, offset
            sq  \\reg, \\offset* {REGBYTES} (sp)
        .endm
        .macro LOAD reg, offset
            lq  \\reg, \\offset* {REGBYTES} (sp)
        .endm"
    };
}

#[cfg(target_pointer_width = "64")]
macro_rules! define_store_load {
    () => {
        ".altmacro
        .macro STORE reg, offset
            sd  \\reg, \\offset* {REGBYTES} (sp)
        .endm
        .macro LOAD reg, offset
            ld  \\reg, \\offset* {REGBYTES} (sp)
        .endm"
    };
}

#[cfg(target_pointer_width = "32")]
macro_rules! define_store_load {
    () => {
        ".altmacro
        .macro STORE reg, offset
            sw  \\reg, \\offset* {REGBYTES} (sp)
        .endm
        .macro LOAD reg, offset
            lw  \\reg, \\offset* {REGBYTES} (sp)
        .endm"
    };
}

#[naked]
#[link_section = ".text"]
unsafe extern "C" fn start_trap() {
    asm!(define_store_load!(), "
    .p2align 2
    addi    sp, sp, -16 * {REGBYTES}
    STORE   ra, 0
    STORE   t0, 1
    STORE   t1, 2
    STORE   t2, 3
    STORE   t3, 4
    STORE   t4, 5
    STORE   t5, 6
    STORE   t6, 7
    STORE   a0, 8
    STORE   a1, 9
    STORE   a2, 10
    STORE   a3, 11
    STORE   a4, 12
    STORE   a5, 13
    STORE   a6, 14
    STORE   a7, 15
    mv      a0, sp
    call    {rust_trap_exception}
    LOAD    ra, 0
    LOAD    t0, 1
    LOAD    t1, 2
    LOAD    t2, 3
    LOAD    t3, 4
    LOAD    t4, 5
    LOAD    t5, 6
    LOAD    t6, 7
    LOAD    a0, 8
    LOAD    a1, 9
    LOAD    a2, 10
    LOAD    a3, 11
    LOAD    a4, 12
    LOAD    a5, 13
    LOAD    a6, 14
    LOAD    a7, 15
    addi    sp, sp, 16 * {REGBYTES}
    sret
    ",
    REGBYTES = const core::mem::size_of::<usize>(),
    rust_trap_exception = sym rust_trap_exception,
    options(noreturn))
}
