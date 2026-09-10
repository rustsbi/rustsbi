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

    // Flatten-save argument registers for non-SBI traps before dispatching,
    // so no later call can clobber them or the ctx pointer.
    let a0 = ctx.a0();
    ctx.regs().a = [a0, a1, a2, a3, a4, a5, a6, a7];

    match cause {
        Trap::Interrupt(interrupt) => handle_interrupt(ctx, interrupt),
        Trap::Exception(exception) => handle_exception(ctx, exception),
    }
}

fn handle_interrupt(
    mut ctx: FastContext,
    interrupt: Interrupt,
) -> FastResult {
    match interrupt {
        Interrupt::MachineSoft => {
            handler::msoft_handler(ctx)
        }
        Interrupt::MachineTimer => {
            use crate::riscv::current_hartid;
            use crate::sbi::features::{Extension, hart_has_extension};
            use crate::sbi::timer;

            unsafe {
                riscv::register::mie::clear_mtimer();
            }
            if !hart_has_extension(current_hartid(), Extension::Sstc) {
                timer::clear();
                unsafe {
                    mip::set_stimer();
                }
            }
            ctx.restore()
        }
        Interrupt::MachineExternal => {
            handler::mext_handler(ctx)
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
            ctx.continue_with(handler::illegal_instruction_handler, ())
        }
        // TODO: Handle Breakpoint
        Exception::Breakpoint => {
            error!("TODO: Unhandled Breakpoint exception");
            unsupported_trap(Some(Trap::Exception(exception)))
        }
        Exception::LoadMisaligned => {
            pmu_firmware_counter_increment(firmware_event::MISALIGNED_LOAD);
            ctx.continue_with(handler::load_misaligned_handler, ())
        }
        Exception::StoreMisaligned => {
            pmu_firmware_counter_increment(firmware_event::MISALIGNED_STORE);
            ctx.continue_with(handler::store_misaligned_handler, ())
        }
        _ => {
            error!("Unhandled exception: {:?}", exception);
            unsupported_trap(Some(Trap::Exception(exception)))
        }
    }
}
