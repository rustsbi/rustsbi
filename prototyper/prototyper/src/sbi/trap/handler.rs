use fast_trap::{EntireContext, EntireContextSeparated, EntireResult, FastContext, FastResult};
use riscv::register::{mepc, mie, mstatus, mtval, satp, sstatus};
use riscv_decode::{Instruction, decode};
use rustsbi::RustSBI;

use crate::platform::PLATFORM;
use crate::riscv::csr::{CSR_TIME, CSR_TIMEH};
use crate::riscv::current_hartid;
use crate::sbi::console;
use crate::sbi::hsm::local_hsm;
use crate::sbi::ipi;
use crate::sbi::rfence;

use super::helper::*;

#[inline]
pub fn switch(mut ctx: FastContext, start_addr: usize, opaque: usize) -> FastResult {
    unsafe {
        sstatus::clear_sie();
        satp::write(0);
    }

    ctx.regs().a[0] = current_hartid();
    ctx.regs().a[1] = opaque;
    ctx.regs().pc = start_addr;
    ctx.call(2)
}

/// Handle machine software inter-processor interrupts.
#[inline]
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
        rfence::rfence_handler();
    }
}

#[inline]
pub fn msoft_handler(ctx: FastContext) -> FastResult {
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
            switch(ctx, next_stage.start_addr, next_stage.opaque)
        }
        // Handle HSM Stop
        Err(rustsbi::spec::hsm::HART_STOP) => {
            ipi::clear_msip();
            unsafe {
                mie::set_msoft();
            }
            riscv::asm::wfi();
            ctx.restore()
        }
        // Handle RFence
        _ => {
            msoft_ipi_handler();
            ctx.restore()
        }
    }
}

#[inline]
#[allow(clippy::too_many_arguments)]
pub fn sbi_call_handler(
    mut ctx: FastContext,
    a1: usize,
    a2: usize,
    a3: usize,
    a4: usize,
    a5: usize,
    a6: usize,
    a7: usize,
) -> FastResult {
    use sbi_spec::{base, hsm, legacy};
    let mut ret = unsafe {
        PLATFORM
            .sbi
            .handle_ecall(a7, a6, [ctx.a0(), a1, a2, a3, a4, a5])
    };
    if ret.is_ok() {
        match (a7, a6) {
            // Handle non-retentive suspend
            (hsm::EID_HSM, hsm::HART_SUSPEND)
                if matches!(ctx.a0() as u32, hsm::suspend_type::NON_RETENTIVE) =>
            {
                return switch(ctx, a1, a2);
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
    let epc = mepc::read();
    mepc::write(epc + get_inst(epc).1);
    ctx.restore()
}

/// Delegate trap handling to supervisor mode.
#[inline]
pub fn delegate(ctx: &mut EntireContextSeparated) {
    use riscv::register::{mcause, scause, sepc, sstatus, stval, stvec};
    unsafe {
        sepc::write(ctx.regs().pc);
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
pub extern "C" fn illegal_instruction_handler(raw_ctx: EntireContext) -> EntireResult {
    let mut ctx = raw_ctx.split().0;

    let inst = decode(mtval::read() as u32);
    match inst {
        Ok(Instruction::Csrrs(csr)) => match csr.csr() {
            CSR_TIME => {
                save_reg_x(
                    &mut ctx,
                    csr.rd() as usize,
                    unsafe { PLATFORM.sbi.ipi.as_ref() }.unwrap().get_time(),
                );
            }
            CSR_TIMEH => {
                save_reg_x(
                    &mut ctx,
                    csr.rd() as usize,
                    unsafe { PLATFORM.sbi.ipi.as_ref() }.unwrap().get_timeh(),
                );
            }
            _ => {
                delegate(&mut ctx);
                return ctx.restore();
            }
        },
        _ => {
            delegate(&mut ctx);
            return ctx.restore();
        }
    }
    let epc = mepc::read();
    mepc::write(epc + get_inst(epc).1);
    ctx.restore()
}

#[inline]
pub extern "C" fn load_misaligned_handler(ctx: EntireContext) -> EntireResult {
    let mut ctx = ctx.split().0;
    let current_pc = mepc::read();
    let current_addr = mtval::read();

    let (current_inst, inst_len) = get_inst(current_pc);
    debug!(
        "Misaligned load: inst/{:x?}, load {:x?} in {:x?}",
        current_inst, current_addr, current_pc
    );
    let decode_result = decode(current_inst as u32);

    // TODO: INST FLD c.*sp
    // TODO: maybe can we reduce the time to update csr for read virtual-address.
    let inst_type = match decode_result {
        Ok(Instruction::Lb(data)) => (data.rd(), VarType::Signed, 1),
        Ok(Instruction::Lbu(data)) => (data.rd(), VarType::UnSigned, 1),
        Ok(Instruction::Lh(data)) => (data.rd(), VarType::Signed, 2),
        Ok(Instruction::Lhu(data)) => (data.rd(), VarType::UnSigned, 2),
        Ok(Instruction::Lw(data)) => (data.rd(), VarType::Signed, 4),
        Ok(Instruction::Lwu(data)) => (data.rd(), VarType::UnSigned, 4),
        Ok(Instruction::Ld(data)) => (data.rd(), VarType::Signed, 8),
        Ok(Instruction::Flw(data)) => (data.rd(), VarType::Float, 4),
        _ => panic!("Unsupported inst"),
    };
    let (target_reg, var_type, len) = inst_type;
    let raw_data = get_data(current_addr, len);
    let read_data = match var_type {
        VarType::Signed => match len {
            1 => raw_data as i8 as usize,
            2 => raw_data as i16 as usize,
            4 => raw_data as i32 as usize,
            8 => raw_data as i64 as usize,
            _ => panic!("Invalid len"),
        },
        VarType::UnSigned => match len {
            1 => raw_data as u8 as usize,
            2 => raw_data as u16 as usize,
            4 => raw_data as u32 as usize,
            8 => raw_data as u64 as usize,
            _ => panic!("Invalid len"),
        },
        VarType::Float => match len {
            // 4 => raw_data as f32 as usize,
            // 8 => raw_data as f64 as usize,
            _ => panic!("Misaligned float is unsupported"),
        },
    };
    debug!(
        "read 0x{:x} from 0x{:x} to x{}, len 0x{:x}",
        read_data, current_addr, target_reg, len
    );
    save_reg_x(&mut ctx, target_reg as usize, read_data);
    mepc::write(current_pc + inst_len);
    ctx.restore()
}

#[inline]
pub extern "C" fn store_misaligned_handler(ctx: EntireContext) -> EntireResult {
    let mut ctx = ctx.split().0;
    let current_pc = mepc::read();
    let current_addr = mtval::read();

    let (current_inst, inst_len) = get_inst(current_pc);
    debug!(
        "Misaligned store: inst/{:x?}, store {:x?} in {:x?}",
        current_inst, current_addr, current_pc
    );

    let decode_result = decode(current_inst as u32);

    // TODO: INST FSD c.*sp
    // TODO: maybe can we reduce the time to update csr for read virtual-address.
    let inst_type = match decode_result {
        Ok(Instruction::Sb(data)) => (data.rs2(), VarType::UnSigned, 1),
        Ok(Instruction::Sh(data)) => (data.rs2(), VarType::UnSigned, 2),
        Ok(Instruction::Sw(data)) => (data.rs2(), VarType::UnSigned, 4),
        Ok(Instruction::Sd(data)) => (data.rs2(), VarType::UnSigned, 8),
        Ok(Instruction::Fsw(data)) => (data.rs2(), VarType::Float, 4),
        _ => panic!("Unsupported inst"),
    };
    let (target_reg, var_type, len) = inst_type;
    let raw_data = get_reg_x(&mut ctx, target_reg as usize);

    // TODO: Float support
    let read_data = match var_type {
        VarType::Signed => match len {
            _ => panic!("Can not store signed data"),
        },
        VarType::UnSigned => match len {
            1 => &(raw_data as u8).to_le_bytes()[..],
            2 => &(raw_data as u16).to_le_bytes()[..],
            4 => &(raw_data as u32).to_le_bytes()[..],
            8 => &(raw_data as u64).to_le_bytes()[..],
            _ => panic!("Invalid len"),
        },
        VarType::Float => match len {
            // 4 => (raw_data as f32).to_le_bytes().to_vec(),
            // 8 => (raw_data as f64).to_le_bytes().to_vec(),
            _ => panic!("Misaligned float is unsupported"),
        },
    };

    debug!(
        "save 0x{:x} to 0x{:x}, len 0x{:x}",
        raw_data, current_addr, len
    );
    for i in 0..read_data.len() {
        save_byte(current_addr + i, read_data[i] as usize);
    }

    mepc::write(current_pc + inst_len);
    ctx.restore()
}
