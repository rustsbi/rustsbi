use crate::riscv::current_hartid;
use crate::sbi::features::{Extension, hart_has_extension};
use crate::sbi::hsm::local_hsm;
use crate::sbi::ipi;
use crate::sbi::trap_stack;
use core::arch::naked_asm;
use riscv::register::{mie, mstatus, satp, sstatus};

/// Boots the next stage on the current hart; never returns.
///
/// Safe wrapper over the naked [`boot_entry`]: the entry diverges (its final
/// `mret` drops to the staged next-mode PC instead of returning), and the
/// staged start address / mode validity is the HSM cell's contract — the
/// HSM cell accepted them before this call.
pub fn boot() -> ! {
    // SAFETY: divergent entry; target validity is the HSM cell's contract.
    unsafe { boot_entry() }
}

/// Boot Function.
/// After boot, this flow will never back again,
/// so we can store a0, a1 and mepc only.
#[unsafe(naked)]
unsafe extern "C" fn boot_entry() -> ! {
    naked_asm!(
        ".align 2",
        // Reset registers before starting a new execution context.
        // This also applies to HSM restart and non-retentive resume; no caller
        // context survives boot_entry. The handoff arguments are loaded below.
        "fence.i",
        "li sp, 0",
        "li gp, 0",
        "li tp, 0",
        "li t0, 0",
        "li t1, 0",
        "li t2, 0",
        "li s0, 0",
        "li s1, 0",
        "li a5, 0",
        "li a6, 0",
        "li a7, 0",
        "li s2, 0",
        "li s3, 0",
        "li s4, 0",
        "li s5, 0",
        "li s6, 0",
        "li s7, 0",
        "li s8, 0",
        "li s9, 0",
        "li s10, 0",
        "li s11, 0",
        "li t3, 0",
        "li t4, 0",
        "li t5, 0",
        "li t6, 0",
        "csrw mscratch, zero",
        // Reset hart local stack
        "call    {locate_stack}",
        "csrw    mscratch, sp",
        // Allocate stack space
        "addi   sp, sp, -{frame_size}",
        // Call handler with context pointer
        "mv     a0, sp",
        "call   {boot_handler}",
        // Restore mepc
        ".if {XLEN} == 64",
        "ld      t0, 0*8(sp)",
        "ld      a0, 1*8(sp)",
        "ld      a1, 2*8(sp)",
        ".else",
        "lw      t0, 0*4(sp)",
        "lw      a0, 1*4(sp)",
        "lw      a1, 2*4(sp)",
        ".endif",
        "csrw    mepc, t0",
        // Restore stack pointer
        "addi    sp, sp, {frame_size}",
        // Switch stacks back
        "csrrw  sp, mscratch, sp",
        // Return from machine mode
        "mret",
        locate_stack = sym trap_stack::locate,
        boot_handler = sym boot_handler,
        XLEN = const usize::BITS,
        frame_size = const size_of::<BootContext>().next_multiple_of(16),
    )
}

pub extern "C" fn boot_handler(ctx: &mut BootContext) {
    #[inline(always)]
    fn boot(ctx: &mut BootContext, start_addr: usize, opaque: usize) {
        unsafe {
            // stvec BASE is four-byte aligned; HSM entry points may only be two-byte aligned.
            if start_addr & 0x3 == 0 {
                core::arch::asm!(
                    "csrw stvec, {start_addr}",
                    start_addr = in(reg) start_addr,
                    options(nomem),
                );
            }
            core::arch::asm!("csrw sscratch, zero", "csrw sie, zero", options(nomem),);
            sstatus::clear_sie();
            satp::write(satp::Satp::from_bits(0));
        }
        ctx.a0 = current_hartid();
        ctx.a1 = opaque;
        ctx.mepc = start_addr;
    }

    match local_hsm().start() {
        Ok(next_stage) => {
            ipi::claim_ipi();
            unsafe {
                mstatus::set_mpie();
                mstatus::set_mpp(next_stage.next_mode);
                mie::set_msoft();
                if !hart_has_extension(current_hartid(), Extension::Sstc) {
                    mie::set_mtimer();
                }
            }
            boot(ctx, next_stage.start_addr, next_stage.opaque);
        }
        Err(rustsbi::spec::hsm::HART_STOP) => {
            ipi::claim_ipi();
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

#[derive(Debug)]
#[repr(C)]
pub struct BootContext {
    pub mepc: usize,
    pub a0: usize,
    pub a1: usize,
}
