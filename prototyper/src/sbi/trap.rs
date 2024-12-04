use core::arch::asm;
use fast_trap::{trap_entry, FastContext, FastResult};
use riscv::register::{
    mcause::{self, Exception as E, Trap as T},
    mepc, mie, mstatus, mtval, satp, sstatus,
};
use rustsbi::RustSBI;

use crate::board::BOARD;
use crate::riscv_spec::{current_hartid, CSR_TIME, CSR_TIMEH};
use crate::sbi::console;
use crate::sbi::hsm::local_hsm;
use crate::sbi::ipi;
use crate::sbi::rfence::{self, local_rfence, RFenceType};

// Constants for page and TLB management
const PAGE_SIZE: usize = 4096;
// TODO: `TLB_FLUSH_LIMIT` is a platform-dependent parameter
const TLB_FLUSH_LIMIT: usize = 4 * PAGE_SIZE;

/// Trap vector table entry point. Maps different trap types to their handlers.
#[naked]
pub(crate) unsafe extern "C" fn trap_vec() {
    asm!(
        ".align 2",
        ".option push",
        ".option norvc",
        "j {default}", // exception
        "j {default}", // supervisor software
        "j {default}", // reserved
        "j {msoft} ",  // machine    software
        "j {default}", // reserved
        "j {default}", // supervisor timer
        "j {default}", // reserved
        "j {mtimer}",  // machine    timer
        "j {default}", // reserved
        "j {default}", // supervisor external
        "j {default}", // reserved
        "j {default}", // machine    external
        ".option pop",
        default = sym trap_entry,
        msoft   = sym msoft,
        mtimer  = sym mtimer,
        options(noreturn)
    )
}

/// Machine timer interrupt handler.
/// Saves context, clears mtimecmp, sets STIP bit, and restores context.
///
/// # Safety
///
/// This is a naked function that directly manipulates registers and stack.
#[naked]
unsafe extern "C" fn mtimer() {
    asm!(
        // Switch stacks: sp <-> mscratch
        "   csrrw sp, mscratch, sp",
        // Save registers to stack
        "   addi   sp, sp, -30*8",
        "   sd     ra, 0*8(sp)
            sd      gp, 2*8(sp)
            sd      tp, 3*8(sp)
            sd      t0, 4*8(sp)
            sd      t1, 5*8(sp)
            sd      t2, 6*8(sp)
            sd      s0, 7*8(sp)
            sd      s1, 8*8(sp)
            sd      a0, 9*8(sp)
            sd      a1, 10*8(sp)
            sd      a2, 11*8(sp)
            sd      a3, 12*8(sp)
            sd      a4, 13*8(sp)
            sd      a5, 14*8(sp)
            sd      a6, 15*8(sp)
            sd      a7, 16*8(sp)
            sd      s2, 17*8(sp)
            sd      s3, 18*8(sp)
            sd      s4, 19*8(sp)
            sd      s5, 20*8(sp)
            sd      s6, 21*8(sp)
            sd      s7, 22*8(sp)
            sd      s8, 23*8(sp)
            sd      s9, 24*8(sp)
            sd     s10, 25*8(sp)
            sd     s11, 26*8(sp)
            sd      t3, 27*8(sp)
            sd      t4, 28*8(sp)
            sd      t5, 29*8(sp)
            sd      t6, 1*8(sp)",
        // Clear machine timer compare register
        "    call  {clear_mtime}",
        // Set supervisor timer interrupt pending bit
        "   li    a0, {mip_stip}
            csrrs zero, mip, a0
        ",
        // Restore registers from stack
        "   ld     ra, 0*8(sp)
            ld      gp, 2*8(sp)
            ld      tp, 3*8(sp)
            ld      t0, 4*8(sp)
            ld      t1, 5*8(sp)
            ld      t2, 6*8(sp)
            ld      s0, 7*8(sp)
            ld      s1, 8*8(sp)
            ld      a0, 9*8(sp)
            ld      a1, 10*8(sp)
            ld      a2, 11*8(sp)
            ld      a3, 12*8(sp)
            ld      a4, 13*8(sp)
            ld      a5, 14*8(sp)
            ld      a6, 15*8(sp)
            ld      a7, 16*8(sp)
            ld      s2, 17*8(sp)
            ld      s3, 18*8(sp)
            ld      s4, 19*8(sp)
            ld      s5, 20*8(sp)
            ld      s6, 21*8(sp)
            ld      s7, 22*8(sp)
            ld      s8, 23*8(sp)
            ld      s9, 24*8(sp)
            ld     s10, 25*8(sp)
            ld     s11, 26*8(sp)
            ld      t3, 27*8(sp)
            ld      t4, 28*8(sp)
            ld      t5, 29*8(sp)
            ld      t6, 1*8(sp)",
        "   addi   sp, sp, 30*8",
        // Switch stacks back: sp <-> mscratch
        "   csrrw sp, mscratch, sp",
        // Return from machine mode
        "   mret",
        mip_stip    = const 1 << 5,
        clear_mtime = sym ipi::clear_mtime,
        options(noreturn)
    )
}

/// Machine software interrupt handler.
///
/// Handles inter-processor interrupts.
#[naked]
pub unsafe extern "C" fn msoft() -> ! {
    asm!(
        ".align 2",
        // Switch stacks
        "csrrw  sp, mscratch, sp",
        // Allocate stack space
        "addi   sp, sp, -32*8",
        // Save registers
        "sd     ra, 0*8(sp)
        sd      gp, 2*8(sp)
        sd      tp, 3*8(sp)
        sd      t0, 4*8(sp)
        sd      t1, 5*8(sp)
        sd      t2, 6*8(sp)
        sd      s0, 7*8(sp)
        sd      s1, 8*8(sp)
        sd      a0, 9*8(sp)
        sd      a1, 10*8(sp)
        sd      a2, 11*8(sp)
        sd      a3, 12*8(sp)
        sd      a4, 13*8(sp)
        sd      a5, 14*8(sp)
        sd      a6, 15*8(sp)
        sd      a7, 16*8(sp)
        sd      s2, 17*8(sp)
        sd      s3, 18*8(sp)
        sd      s4, 19*8(sp)
        sd      s5, 20*8(sp)
        sd      s6, 21*8(sp)
        sd      s7, 22*8(sp)
        sd      s8, 23*8(sp)
        sd      s9, 24*8(sp)
        sd     s10, 25*8(sp)
        sd     s11, 26*8(sp)
        sd      t3, 27*8(sp)
        sd      t4, 28*8(sp)
        sd      t5, 29*8(sp)
        sd      t6, 30*8(sp)",
        // Save mepc and mscratch
        "csrr   t0, mepc
        sd      t0, 31*8(sp)",
        "csrr   t2, mscratch",
        "sd     t2, 1*8(sp)",
        // Call handler with context pointer
        "mv     a0, sp",
        "call   {msoft_handler}",
        // Restore mepc
        "ld     t0, 31*8(sp)
        csrw    mepc, t0",
        // Restore registers
        "ld     ra, 0*8(sp)
        ld      gp, 2*8(sp)
        ld      tp, 3*8(sp)
        ld      t0, 4*8(sp)
        ld      t1, 5*8(sp)
        ld      t2, 6*8(sp)
        ld      s0, 7*8(sp)
        ld      s1, 8*8(sp)
        ld      a0, 9*8(sp)
        ld      a1, 10*8(sp)
        ld      a2, 11*8(sp)
        ld      a3, 12*8(sp)
        ld      a4, 13*8(sp)
        ld      a5, 14*8(sp)
        ld      a6, 15*8(sp)
        ld      a7, 16*8(sp)
        ld      s2, 17*8(sp)
        ld      s3, 18*8(sp)
        ld      s4, 19*8(sp)
        ld      s5, 20*8(sp)
        ld      s6, 21*8(sp)
        ld      s7, 22*8(sp)
        ld      s8, 23*8(sp)
        ld      s9, 24*8(sp)
        ld     s10, 25*8(sp)
        ld     s11, 26*8(sp)
        ld      t3, 27*8(sp)
        ld      t4, 28*8(sp)
        ld      t5, 29*8(sp)
        ld      t6, 30*8(sp)",
        // Restore stack pointer
        "addi   sp, sp, 32*8",
        // Switch stacks back
        "csrrw  sp, mscratch, sp",
        // Return from machine mode
        "mret",
        msoft_handler = sym msoft_handler,
        options(noreturn)
    );
}

/// Machine software interrupt handler implementation.
///
/// Handles HSM (Hart State Management) and RFence operations.
pub extern "C" fn msoft_handler(ctx: &mut SupervisorContext) {
    #[inline(always)]
    fn boot(ctx: &mut SupervisorContext, start_addr: usize, opaque: usize) {
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
        // Handle RFence
        _ => {
            msoft_ipi_handler();
        }
    }
}

/// Handles a single remote fence operation.
pub fn rfence_single_handler() {
    let rfence_context = local_rfence().unwrap().get();
    if let Some((ctx, id)) = rfence_context {
        match ctx.op {
            // Handle instruction fence
            RFenceType::FenceI => unsafe {
                asm!("fence.i");
                rfence::remote_rfence(id).unwrap().sub();
            },
            // Handle virtual memory address fence
            RFenceType::SFenceVma => {
                // If the flush size is greater than the maximum limit then simply flush all
                if (ctx.start_addr == 0 && ctx.size == 0)
                    || (ctx.size == usize::MAX)
                    || (ctx.size > TLB_FLUSH_LIMIT)
                {
                    unsafe {
                        asm!("sfence.vma");
                    }
                } else {
                    for offset in (0..ctx.size).step_by(PAGE_SIZE) {
                        let addr = ctx.start_addr + offset;
                        unsafe {
                            asm!("sfence.vma {}", in(reg) addr);
                        }
                    }
                }
                rfence::remote_rfence(id).unwrap().sub();
            }
            // Handle virtual memory address fence with ASID
            RFenceType::SFenceVmaAsid => {
                let asid = ctx.asid;
                // If the flush size is greater than the maximum limit then simply flush all
                if (ctx.start_addr == 0 && ctx.size == 0)
                    || (ctx.size == usize::MAX)
                    || (ctx.size > TLB_FLUSH_LIMIT)
                {
                    unsafe {
                        asm!("sfence.vma {}, {}", in(reg) 0, in(reg) asid);
                    }
                } else {
                    for offset in (0..ctx.size).step_by(PAGE_SIZE) {
                        let addr = ctx.start_addr + offset;
                        unsafe {
                            asm!("sfence.vma {}, {}", in(reg) addr, in(reg) asid);
                        }
                    }
                }
                rfence::remote_rfence(id).unwrap().sub();
            }
            rfencetype => {
                error!("Unsupported RFence Type: {:?}!", rfencetype);
            }
        }
    }
}

/// Process all pending remote fence operations.
pub fn rfence_handler() {
    while !local_rfence().unwrap().is_empty() {
        rfence_single_handler();
    }
}

/// Handle machine software inter-processor interrupts.
pub fn msoft_ipi_handler() {
    use ipi::get_and_reset_ipi_type;
    ipi::clear_msip();
    let ipi_type = get_and_reset_ipi_type();
    // Handle supervisor software interrupt
    if (ipi_type & ipi::IPI_TYPE_SSOFT) != 0 {
        unsafe {
            riscv::register::mip::set_ssoft();
        }
    }
    // Handle fence operation
    if (ipi_type & ipi::IPI_TYPE_FENCE) != 0 {
        rfence_handler();
    }
}

/// Fast trap handler for SBI calls and illegal instructions.
pub extern "C" fn fast_handler(
    mut ctx: FastContext,
    a1: usize,
    a2: usize,
    a3: usize,
    a4: usize,
    a5: usize,
    a6: usize,
    a7: usize,
) -> FastResult {
    #[inline]
    fn resume(mut ctx: FastContext, start_addr: usize, opaque: usize) -> FastResult {
        unsafe {
            sstatus::clear_sie();
            satp::write(0);
        }
        ctx.regs().a[0] = current_hartid();
        ctx.regs().a[1] = opaque;
        ctx.regs().pc = start_addr;
        ctx.call(2)
    }
    match mcause::read().cause() {
        // Handle SBI calls
        T::Exception(E::SupervisorEnvCall) => {
            use sbi_spec::{base, hsm, legacy};
            let mut ret = unsafe {
                BOARD
                    .sbi
                    .handle_ecall(a7, a6, [ctx.a0(), a1, a2, a3, a4, a5])
            };
            if ret.is_ok() {
                match (a7, a6) {
                    // Handle non-retentive suspend
                    (hsm::EID_HSM, hsm::HART_SUSPEND)
                        if matches!(ctx.a0() as u32, hsm::suspend_type::NON_RETENTIVE) =>
                    {
                        return resume(ctx, a1, a2);
                    }
                    // Handle legacy console probe
                    (base::EID_BASE, base::PROBE_EXTENSION)
                        if matches!(
                            ctx.a0(),
                            legacy::LEGACY_CONSOLE_PUTCHAR | legacy::LEGACY_CONSOLE_GETCHAR
                        ) =>
                    {
                        ret.value = 1;
                    }
                    _ => {}
                }
            } else {
                match a7 {
                    legacy::LEGACY_CONSOLE_PUTCHAR => {
                        ret.error = console::putchar(ctx.a0());
                        ret.value = a1;
                    }
                    legacy::LEGACY_CONSOLE_GETCHAR => {
                        ret.error = console::getchar();
                        ret.value = a1;
                    }
                    _ => {}
                }
            }
            ctx.regs().a = [ret.error, ret.value, a2, a3, a4, a5, a6, a7];
            mepc::write(mepc::read() + 4);
            ctx.restore()
        }
        // Handle illegal instructions
        T::Exception(E::IllegalInstruction) => {
            if mstatus::read().mpp() == mstatus::MPP::Machine {
                panic!("Cannot handle illegal instruction exception from M-MODE");
            }

            ctx.regs().a = [ctx.a0(), a1, a2, a3, a4, a5, a6, a7];
            if !illegal_instruction_handler(&mut ctx) {
                delegate();
            }
            ctx.restore()
        }
        // Handle other traps
        trap => {
            println!(
                "
-----------------------------
> trap:    {trap:?}
> mepc:    {:#018x}
> mtval:   {:#018x}
-----------------------------
            ",
                mepc::read(),
                mtval::read()
            );
            panic!("Stopped with unsupported trap")
        }
    }
}

/// Delegate trap handling to supervisor mode.
#[inline]
fn delegate() {
    use riscv::register::{mcause, mepc, mtval, scause, sepc, sstatus, stval, stvec};
    unsafe {
        // TODO: 当支持中断嵌套时，需要从ctx里获取mpec。当前ctx.reg().pc与mepc不一致
        sepc::write(mepc::read());
        scause::write(mcause::read().bits());
        stval::write(mtval::read());
        sstatus::clear_sie();
        if mstatus::read().mpp() == mstatus::MPP::Supervisor {
            sstatus::set_spp(sstatus::SPP::Supervisor);
        } else {
            sstatus::set_spp(sstatus::SPP::User);
        }
        mstatus::set_mpp(mstatus::MPP::Supervisor);
        mepc::write(stvec::read().address());
    }
}

/// Handle illegal instructions, particularly CSR access.
#[inline]
fn illegal_instruction_handler(ctx: &mut FastContext) -> bool {
    use riscv::register::{mepc, mtval};
    use riscv_decode::{decode, Instruction};

    let inst = decode(mtval::read() as u32);
    match inst {
        Ok(Instruction::Csrrs(csr)) => match csr.csr() {
            CSR_TIME => {
                assert!(
                    10 <= csr.rd() && csr.rd() <= 17,
                    "Unsupported CSR rd: {}",
                    csr.rd()
                );
                ctx.regs().a[(csr.rd() - 10) as usize] =
                    unsafe { BOARD.sbi.ipi.as_ref() }.unwrap().get_time();
            }
            CSR_TIMEH => {
                assert!(
                    10 <= csr.rd() && csr.rd() <= 17,
                    "Unsupported CSR rd: {}",
                    csr.rd()
                );
                ctx.regs().a[(csr.rd() - 10) as usize] =
                    unsafe { BOARD.sbi.ipi.as_ref() }.unwrap().get_timeh();
            }
            _ => return false,
        },
        _ => return false,
    }
    mepc::write(mepc::read() + 4);
    true
}

/// Supervisor context structure containing saved register state.
#[derive(Debug)]
#[repr(C)]
pub struct SupervisorContext {
    pub ra: usize, // 0
    pub sp: usize,
    pub gp: usize,
    pub tp: usize,
    pub t0: usize,
    pub t1: usize,
    pub t2: usize,
    pub s0: usize,
    pub s1: usize,
    pub a0: usize,
    pub a1: usize,
    pub a2: usize,
    pub a3: usize,
    pub a4: usize,
    pub a5: usize,
    pub a6: usize,
    pub a7: usize,
    pub s2: usize,
    pub s3: usize,
    pub s4: usize,
    pub s5: usize,
    pub s6: usize,
    pub s7: usize,
    pub s8: usize,
    pub s9: usize,
    pub s10: usize,
    pub s11: usize,
    pub t3: usize,
    pub t4: usize,
    pub t5: usize,
    pub t6: usize,   // 30
    pub mepc: usize, // 31
}
