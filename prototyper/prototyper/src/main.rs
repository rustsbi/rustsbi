#![feature(alloc_error_handler)]
#![feature(naked_functions)]
#![feature(fn_align)]
#![no_std]
#![no_main]
#![allow(static_mut_refs)]

extern crate alloc;
#[macro_use]
extern crate log;
#[macro_use]
mod macros;

mod cfg;
mod devicetree;
mod fail;
mod firmware;
mod platform;
mod riscv;
mod sbi;

use core::arch::{asm, naked_asm};

use crate::platform::PLATFORM;
use crate::riscv::csr::menvcfg;
use crate::riscv::current_hartid;
use crate::sbi::extensions::{
    Extension, PrivilegedVersion, hart_extension_probe, hart_privileged_version,
    privileged_version_detection,
};
use crate::sbi::hart_context::NextStage;
use crate::sbi::heap::sbi_heap_init;
use crate::sbi::hsm::local_remote_hsm;
use crate::sbi::ipi;
use crate::sbi::trap;
use crate::sbi::trap_stack;

pub const R_RISCV_RELATIVE: usize = 3;

#[unsafe(no_mangle)]
extern "C" fn rust_main(_hart_id: usize, opaque: usize, nonstandard_a2: usize) {
    // Track whether SBI is initialized and ready.

    let boot_hart_info = firmware::get_boot_hart(opaque, nonstandard_a2);
    // boot hart task entry.
    if boot_hart_info.is_boot_hart {
        // Initialize the sbi heap
        sbi_heap_init();

        // parse the device tree
        let fdt_address = boot_hart_info.fdt_address;

        unsafe {
            PLATFORM.init(fdt_address);
            PLATFORM.print_board_info();
        }

        firmware::set_pmp(unsafe { PLATFORM.info.memory_range.as_ref().unwrap() });
        firmware::log_pmp_cfg(unsafe { PLATFORM.info.memory_range.as_ref().unwrap() });

        // Get boot information and prepare for kernel entry.
        let boot_info = firmware::get_boot_info(nonstandard_a2);
        let (mpp, next_addr) = (boot_info.mpp, boot_info.next_address);

        // Log boot hart ID and PMP information
        let hart_id = current_hartid();
        info!("{:<30}: {}", "Boot HART ID", hart_id);

        // Detection Priv Version
        privileged_version_detection();
        let priv_version = hart_privileged_version(hart_id);
        info!("{:<30}: {:?}", "Boot HART Privileged Version", priv_version);

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
        // Other harts task entry.
        trap_stack::prepare_for_trap();

        // Wait for boot hart to complete SBI initialization.
        while !unsafe { PLATFORM.ready() } {
            core::hint::spin_loop()
        }

        firmware::set_pmp(unsafe { PLATFORM.info.memory_range.as_ref().unwrap() });
        // Detection Priv Version
        privileged_version_detection();
    }
    // Clear all pending IPIs.
    ipi::clear_all();

    // Configure CSRs and trap handling.
    unsafe {
        // Delegate all interrupts and exceptions to supervisor mode.
        asm!("csrw mideleg,    {}", in(reg) !0);
        asm!("csrw medeleg,    {}", in(reg) !0);
        asm!("csrw mcounteren, {}", in(reg) !0);
        asm!("csrw scounteren, {}", in(reg) !0);
        use ::riscv::register::{medeleg, mtvec};
        // Keep supervisor environment calls and illegal instructions in M-mode.
        medeleg::clear_supervisor_env_call();
        medeleg::clear_load_misaligned();
        medeleg::clear_store_misaligned();
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
        // Set up trap handling.
        mtvec::write(fast_trap::trap_entry as _, mtvec::TrapMode::Direct);
    }
}

#[naked]
#[unsafe(link_section = ".text.entry")]
#[unsafe(export_name = "_start")]
unsafe extern "C" fn start() -> ! {
    unsafe {
        naked_asm!(
            ".option arch, +a",
            // 1. Turn off interrupt.
            "
            csrw    mie, zero",
            // 2. Initialize programming language runtime.
            // only clear bss if hartid matches preferred boot hart id.
            // Race
            "
            lla      t0, 6f
            li       t1, 1
            amoadd.w t0, t1, 0(t0)
            bnez     t0, 4f
            call     {relocation_update}",
            // 3. Boot hart clear bss segment.
            "1:
            lla     t0, sbi_bss_start
            lla     t1, sbi_bss_end",
            "2:
            bgeu    t0, t1, 3f
            sd      zero, 0(t0)
            addi    t0, t0, 8
            j       2b",
            // 3.1 Boot hart set bss ready signal.
            "3:
            lla     t0, 7f
            li      t1, 1
            amoadd.w t0, t1, 0(t0)
            j       5f",
            // 3.2 Other harts are waiting for bss ready signal.
            "4:
            lla     t0, 7f
            lw      t0, 0(t0)
            beqz    t0, 4b",
            // 4. Prepare stack for each hart.
            "5:
            call    {locate_stack}
            call    {main}
            csrw    mscratch, sp
            j       {hart_boot}
            .balign  4",
            "6:", // boot hart race signal.
            "  .word    0",
            "7:", // bss ready signal.
            "  .word    0",
            relocation_update = sym relocation_update,
            locate_stack = sym trap_stack::locate,
            main         = sym rust_main,
            hart_boot    = sym trap::boot::boot,
        )
    }
}

// Handle relocations for position-independent code
#[naked]
unsafe extern "C" fn relocation_update() {
    unsafe {
        naked_asm!(
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
            "   fence.i",

            // Return
            "   ret",
            R_RISCV_RELATIVE = const R_RISCV_RELATIVE,
            START_ADDRESS = const cfg::SBI_LINK_START_ADDRESS,
        )
    }
}
