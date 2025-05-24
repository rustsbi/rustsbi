use crate::riscv::current_hartid;
use crate::sbi::hsm::local_hsm;
use crate::sbi::ipi;
use crate::sbi::trap_stack;
use core::arch::naked_asm;
use riscv::register::{mie, mstatus, satp, sstatus};

/// Boot Function.
/// After boot, this flow will never back again,
/// so we can store a0, a1 and mepc only.
#[unsafe(naked)]
pub unsafe extern "C" fn boot() -> ! {
    naked_asm!(
        ".align 2",
        // Reset hart local stack
        "call    {locate_stack}",
        "csrw    mscratch, sp",
        // Allocate stack space
        "addi   sp, sp, -3*8",
        // Call handler with context pointer
        "mv     a0, sp",
        "call   {boot_handler}",
        // Restore mepc
        "ld     t0, 0*8(sp)
        csrw    mepc, t0",
        // Restore registers
        "ld      a0, 1*8(sp)",
        "ld      a1, 2*8(sp)",
        // Restore stack pointer
        "add     sp, sp, 3*8",
        // Switch stacks back
        "csrrw  sp, mscratch, sp",
        // Return from machine mode
        "mret",
        locate_stack = sym trap_stack::locate,
        boot_handler = sym boot_handler,
    );
}

/// Boot Handler.
pub extern "C" fn boot_handler(ctx: &mut BootContext) {
    #[inline(always)]
    fn boot(ctx: &mut BootContext, start_addr: usize, opaque: usize) {
        unsafe {
            sstatus::clear_sie();
            satp::write(0);
        }
        ctx.a0 = current_hartid();
        ctx.a1 = opaque;
        ctx.mepc = start_addr;
    }

    match local_hsm().start() {
        // Handle HSM Start
        Ok(next_stage) => {
            ipi::clear_msip();
            unsafe {
                mstatus::set_mpie();
                mstatus::set_mpp(next_stage.next_mode);
                mie::set_msoft();
                mie::set_mtimer();
            }
            boot(ctx, next_stage.start_addr, next_stage.opaque);
        }
        // Handle HSM Stop
        Err(rustsbi::spec::hsm::HART_STOP) => {
            ipi::clear_msip();
            unsafe {
                mie::set_msoft();
            }
            riscv::asm::wfi();
        }
        _ => {
            unreachable!("Boot stage hsm should be start or stop.");
        }
    }
}

/// Boot context structure containing saved register state.
#[derive(Debug)]
#[repr(C)]
pub struct BootContext {
    pub mepc: usize, // 0
    pub a0: usize,
    pub a1: usize, // 2
}
