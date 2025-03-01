use fast_trap::{FastContext, FastResult};
use riscv::register::{mepc, mie, mstatus, satp, sstatus};
use rustsbi::RustSBI;

use crate::platform::PLATFORM;
use crate::riscv::csr::{CSR_TIME, CSR_TIMEH};
use crate::riscv::current_hartid;
use crate::sbi::console;
use crate::sbi::hsm::local_hsm;
use crate::sbi::ipi;
use crate::sbi::rfence;

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
    mepc::write(mepc::read() + 4);
    ctx.restore()
}

/// Delegate trap handling to supervisor mode.
#[inline]
pub fn delegate(ctx: &mut FastContext) {
    use riscv::register::{mcause, mepc, mtval, scause, sepc, sstatus, stval, stvec};
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
pub fn illegal_instruction_handler(ctx: &mut FastContext) -> bool {
    use riscv::register::{mepc, mtval};
    use riscv_decode::{Instruction, decode};

    let inst = decode(mtval::read() as u32);
    match inst {
        Ok(Instruction::Csrrs(csr)) => match csr.csr() as u16 {
            CSR_TIME => {
                assert!(
                    10 <= csr.rd() && csr.rd() <= 17,
                    "Unsupported CSR rd: {}",
                    csr.rd()
                );
                ctx.regs().a[(csr.rd() - 10) as usize] =
                    unsafe { PLATFORM.sbi.ipi.as_ref() }.unwrap().get_time();
            }
            CSR_TIMEH => {
                assert!(
                    10 <= csr.rd() && csr.rd() <= 17,
                    "Unsupported CSR rd: {}",
                    csr.rd()
                );
                ctx.regs().a[(csr.rd() - 10) as usize] =
                    unsafe { PLATFORM.sbi.ipi.as_ref() }.unwrap().get_timeh();
            }
            _ => return false,
        },
        _ => return false,
    }
    mepc::write(mepc::read() + 4);
    true
}
