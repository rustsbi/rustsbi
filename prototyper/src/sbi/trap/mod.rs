pub mod boot;
pub mod handler;

use fast_trap::{FastContext, FastResult};
use riscv::register::{
    mcause::{self, Exception as E, Interrupt, Trap as T},
    mepc, mip, mstatus, mtval,
};

/// Fast trap handler for all trap.
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
    // Save mepc into context
    ctx.regs().pc = mepc::read();

    let save_regs = |ctx: &mut FastContext| {
        ctx.regs().a = [ctx.a0(), a1, a2, a3, a4, a5, a6, a7];
    };

    match mcause::read().cause() {
        // Handle Msoft
        T::Interrupt(Interrupt::MachineSoft) => {
            save_regs(&mut ctx);
            handler::msoft_handler(ctx)
        }
        // Handle MTimer
        T::Interrupt(Interrupt::MachineTimer) => {
            use crate::sbi::ipi;

            ipi::clear_mtime();
            unsafe {
                mip::clear_stimer();
            }
            save_regs(&mut ctx);
            ctx.restore()
        }
        // Handle SBI calls
        T::Exception(E::SupervisorEnvCall) => {
            handler::sbi_call_handler(ctx, a1, a2, a3, a4, a5, a6, a7)
        }
        // Handle illegal instructions
        T::Exception(E::IllegalInstruction) => {
            if mstatus::read().mpp() == mstatus::MPP::Machine {
                panic!("Cannot handle illegal instruction exception from M-MODE");
            }

            save_regs(&mut ctx);
            if !handler::illegal_instruction_handler(&mut ctx) {
                handler::delegate(&mut ctx);
            }
            ctx.restore()
        }
        // Handle other traps
        trap => {
            error!("-----------------------------");
            error!("trap:    {trap:?}");
            error!("mepc:    {:#018x}", mepc::read());
            error!("mtval:   {:#018x}", mtval::read());
            error!("-----------------------------");
            panic!("Stopped with unsupported trap")
        }
    }
}
