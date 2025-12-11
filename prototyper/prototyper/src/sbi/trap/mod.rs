pub mod boot;
pub mod handler;

mod helper;

use super::pmu::pmu_firmware_counter_increment;
use crate::fail::unsupported_trap;

use fast_trap::{FastContext, FastResult};
use riscv::interrupt::machine::{Exception, Interrupt};
use riscv::register::{
    mcause::{self, Trap},
    mepc, mip, mstatus,
};
use sbi_spec::pmu::firmware_event;

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

    let cause = match mcause::read().cause().try_into() {
        Ok(cause) => cause,
        Err(err) => {
            error!("Failed to parse mcause: {:?}", err);
            unsupported_trap(None)
        }
    };

    // Fast path for SBI calls
    if let Trap::Exception(Exception::SupervisorEnvCall) = cause {
        return handler::sbi_call_handler(ctx, a1, a2, a3, a4, a5, a6, a7);
    }

    // Save registers for other traps
    let save_regs = |ctx: &mut FastContext| {
        ctx.regs().a = [ctx.a0(), a1, a2, a3, a4, a5, a6, a7];
    };

    match cause {
        Trap::Interrupt(interrupt) => handle_interrupt(ctx, interrupt, save_regs),
        Trap::Exception(exception) => handle_exception(ctx, exception, save_regs),
    }
}

fn handle_interrupt(
    mut ctx: FastContext,
    interrupt: Interrupt,
    save_regs: impl Fn(&mut FastContext),
) -> FastResult {
    match interrupt {
        Interrupt::MachineSoft => {
            save_regs(&mut ctx);
            handler::msoft_handler(ctx)
        }
        Interrupt::MachineTimer => {
            use crate::sbi::ipi;

            ipi::clear_mtime();
            unsafe {
                mip::set_stimer();
            }
            save_regs(&mut ctx);
            ctx.restore()
        }
        // TODO: Handle MachineExternal
        Interrupt::MachineExternal => {
            error!("TODO: Unhandled MachineExternal interrupt");
            unsupported_trap(Some(Trap::Interrupt(interrupt)))
        }
        _ => {
            error!("Unhandled interrupt: {:?}", interrupt);
            unsupported_trap(Some(Trap::Interrupt(interrupt)))
        }
    }
}

fn handle_exception(
    mut ctx: FastContext,
    exception: Exception,
    save_regs: impl Fn(&mut FastContext),
) -> FastResult {
    match exception {
        // TODO: Handle InstructionMisaligned
        Exception::InstructionMisaligned => {
            error!("TODO: Unhandled InstructionMisaligned exception");
            unsupported_trap(Some(Trap::Exception(exception)))
        }
        Exception::IllegalInstruction => {
            pmu_firmware_counter_increment(firmware_event::ILLEGAL_INSN);
            if mstatus::read().mpp() == mstatus::MPP::Machine {
                panic!("Cannot handle illegal instruction exception from M-MODE");
            }
            save_regs(&mut ctx);
            ctx.continue_with(handler::illegal_instruction_handler, ())
        }
        // TODO: Handle Breakpoint
        Exception::Breakpoint => {
            error!("TODO: Unhandled Breakpoint exception");
            unsupported_trap(Some(Trap::Exception(exception)))
        }
        Exception::LoadMisaligned => {
            pmu_firmware_counter_increment(firmware_event::MISALIGNED_LOAD);
            save_regs(&mut ctx);
            ctx.continue_with(handler::load_misaligned_handler, ())
        }
        Exception::StoreMisaligned => {
            pmu_firmware_counter_increment(firmware_event::MISALIGNED_STORE);
            save_regs(&mut ctx);
            ctx.continue_with(handler::store_misaligned_handler, ())
        }
        _ => {
            error!("Unhandled exception: {:?}", exception);
            unsupported_trap(Some(Trap::Exception(exception)))
        }
    }
}
