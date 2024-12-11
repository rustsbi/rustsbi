#![feature(naked_functions)]
#![feature(fn_align)]
#![no_std]
#![no_main]
#![allow(static_mut_refs)]

#[macro_use]
extern crate log;
#[macro_use]
mod macros;

mod board;
mod dt;
mod fail;
mod firmware;
mod riscv_spec;
mod sbi;

use core::arch::asm;

use crate::board::BOARD;
use crate::riscv_spec::{current_hartid, menvcfg};
use crate::sbi::extensions::{
    hart_extension_probe, hart_privileged_version, privileged_version_detection, Extension,
    PrivilegedVersion,
};
use crate::sbi::hart_context::NextStage;
use crate::sbi::hsm::local_remote_hsm;
use crate::sbi::ipi;
use crate::sbi::trap::{self, trap_vec};
use crate::sbi::trap_stack;

pub const START_ADDRESS: usize = 0x80000000;
pub const R_RISCV_RELATIVE: usize = 3;

#[no_mangle]
extern "C" fn rust_main(_hart_id: usize, opaque: usize, nonstandard_a2: usize) {
    // Track whether SBI is initialized and ready.

    let boot_hart_info = firmware::get_boot_hart(opaque, nonstandard_a2);
    // boot hart task entry.
    if boot_hart_info.is_boot_hart {
        // parse the device tree
        let fdt_address = boot_hart_info.fdt_address;

        unsafe {
            BOARD.init(fdt_address);
            BOARD.print_board_info();
        }
        firmware::set_pmp(unsafe { BOARD.info.memory_range.as_ref().unwrap() });

        // Get boot information and prepare for kernel entry.
        let boot_info = firmware::get_boot_info(nonstandard_a2);
        let (mpp, next_addr) = (boot_info.mpp, boot_info.next_address);

        // Start kernel.
        local_remote_hsm().start(NextStage {
            start_addr: next_addr,
            next_mode: mpp,
            opaque: fdt_address,
        });

        info!(
            "Redirecting hart {} to 0x{:0>16x} in {:?} mode.",
            current_hartid(),
            next_addr,
            mpp
        );
    } else {
        // 设置陷入栈
        trap_stack::prepare_for_trap();

        // Wait for boot hart to complete SBI initialization.
        while !unsafe { BOARD.ready() } {
            core::hint::spin_loop()
        }

        firmware::set_pmp(unsafe { BOARD.info.memory_range.as_ref().unwrap() });
    }

    // Detection Priv Ver
    privileged_version_detection();
    // Clear all pending IPIs.
    ipi::clear_all();

    // Configure CSRs and trap handling.
    unsafe {
        // Delegate all interrupts and exceptions to supervisor mode.
        asm!("csrw mideleg,    {}", in(reg) !0);
        asm!("csrw medeleg,    {}", in(reg) !0);
        asm!("csrw mcounteren, {}", in(reg) !0);
        asm!("csrw scounteren, {}", in(reg) !0);
        use riscv::register::{medeleg, mtvec};
        // Keep supervisor environment calls and illegal instructions in M-mode.
        medeleg::clear_supervisor_env_call();
        medeleg::clear_illegal_instruction();
        if hart_privileged_version(current_hartid()) >= PrivilegedVersion::Version1_12 {
            // Configure environment features based on available extensions.
            if hart_extension_probe(current_hartid(), Extension::Sstc) {
                menvcfg::set_bits(
                    menvcfg::STCE | menvcfg::CBIE_INVALIDATE | menvcfg::CBCFE | menvcfg::CBZE,
                );
            } else {
                menvcfg::set_bits(menvcfg::CBIE_INVALIDATE | menvcfg::CBCFE | menvcfg::CBZE);
            }
        }
        // Set up vectored trap handling.
        mtvec::write(trap_vec as _, mtvec::TrapMode::Vectored);
    }
}

#[naked]
#[link_section = ".text.entry"]
#[export_name = "_start"]
unsafe extern "C" fn start() -> ! {
    core::arch::asm!(
        // 1. Turn off interrupt.
        "   csrw    mie, zero",
        // 2. Initialize programming language runtime.
        // only clear bss if hartid matches preferred boot hart id.
        "   csrr    t0, mhartid",
        "   bne     t0, zero, 4f",
        "   call    {relocation_update}",
        "1:",
        // 3. Hart 0 clear bss segment.
        "   lla     t0, sbi_bss_start
            lla     t1, sbi_bss_end
         2: bgeu    t0, t1, 3f
            sd      zero, 0(t0)
            addi    t0, t0, 8
            j       2b",
        "3: ", // Hart 0 set bss ready signal.
        "   lla     t0, 6f
            li      t1, 1
            amoadd.w t0, t1, 0(t0)
            j       5f",
        "4:", // Other harts are waiting for bss ready signal.
        "   li      t1, 1
            lla     t0, 6f
            lw      t0, 0(t0)
            bne     t0, t1, 4b",
        "5:",
         // 4. Prepare stack for each hart.
        "   call    {locate_stack}",
        "   call    {main}",
        "   csrw    mscratch, sp",
        "   j       {hart_boot}",
        "  .balign  4",
        "6:",  // bss ready signal.
        "  .word    0",
        relocation_update = sym relocation_update,
        locate_stack = sym trap_stack::locate,
        main         = sym rust_main,
        hart_boot    = sym trap::msoft,
        options(noreturn)
    )
}

// Handle relocations for position-independent code
#[naked]
unsafe extern "C" fn relocation_update() {
    asm!(
        // Get load offset.
        "   li t0, {START_ADDRESS}",
        "   lla t1, sbi_start",
        "   sub t2, t1, t0",

        // Foreach rela.dyn and update relocation.
        "   lla t0, __rel_dyn_start",
        "   lla t1, __rel_dyn_end",
        "   li  t3, {R_RISCV_RELATIVE}",
        "1:",
        "   ld  t4, 8(t0)",
        "   bne t4, t3, 2f",
        "   ld t4, 0(t0)", // Get offset
        "   ld t5, 16(t0)", // Get append
        "   add t4, t4, t2", // Add load offset to offset add append
        "   add t5, t5, t2",
        "   sd t5, 0(t4)", // Update address
        "   addi t0, t0, 24", // Get next rela item
        "2:",
        "   blt t0, t1, 1b",

        // Return
        "   ret",
        R_RISCV_RELATIVE = const R_RISCV_RELATIVE,
        START_ADDRESS = const START_ADDRESS,
        options(noreturn)
    )
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    use riscv::register::*;
    error!("Hart {} {info}", riscv::register::mhartid::read());
    error!("-----------------------------");
    error!("mcause:  {:?}", mcause::read().cause());
    error!("mepc:    {:#018x}", mepc::read());
    error!("mtval:   {:#018x}", mtval::read());
    error!("-----------------------------");
    error!("System shutdown scheduled due to RustSBI panic");
    loop {}
}
