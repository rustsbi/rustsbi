use aclint::SifiveClint as Clint;
use core::arch::asm;
use fast_trap::{trap_entry, FastContext, FastResult};
use riscv::register::{
    mepc, mie, mstatus};
use rustsbi::RustSBI;

use crate::board::SBI;
use crate::clint::{self, SIFIVECLINT};
use crate::console::{MachineConsole, CONSOLE};
use crate::hart_id;
use crate::hsm::local_hsm;
use crate::riscv_spec::{CSR_TIME, CSR_TIMEH};

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
        mtimer  = sym mtimer,
        msoft   = sym msoft,
        options(noreturn)
    )
}

/// machine timer 中断代理
///
/// # Safety
///
/// 裸函数。
#[naked]
unsafe extern "C" fn mtimer() {
    asm!(
        // 换栈：
        // sp      : M sp
        // mscratch: S sp
        "   csrrw sp, mscratch, sp",
        // 保护
        "   addi  sp, sp, -4*8
            sd    ra, 0*8(sp)
            sd    a0, 1*8(sp)
            sd    a1, 2*8(sp)
            sd    a2, 3*8(sp)
        ",
        // 清除 mtimecmp
        "   la    a0, {clint_ptr}
            ld    a0, (a0)
            csrr  a1, mhartid
            addi  a2, zero, -1
            call  {set_mtimecmp}
        ",
        // 设置 stip
        "   li    a0, {mip_stip}
            csrrs zero, mip, a0
        ",
        // 恢复
        "   ld    ra, 0*8(sp)
            ld    a0, 1*8(sp)
            ld    a1, 2*8(sp)
            ld    a2, 3*8(sp)
            addi  sp, sp,  4*8
        ",
        // 换栈：
        // sp      : S sp
        // mscratch: M sp
        "   csrrw sp, mscratch, sp",
        // 返回
        "   mret",
        mip_stip     = const 1 << 5,
        clint_ptr    =   sym SIFIVECLINT,
        //                   Clint::write_mtimecmp_naked(&self, hart_idx, val)
        set_mtimecmp =   sym Clint::write_mtimecmp_naked,
        options(noreturn)
    )
}

/// machine soft 中断代理
///
/// # Safety
///
/// 裸函数。
#[naked]
unsafe extern "C" fn msoft() {
    asm!(
        // 换栈：
        // sp      : M sp
        // mscratch: S sp
        "   csrrw sp, mscratch, sp",
        // 保护
        "   addi sp, sp, -3*8
            sd   ra, 0*8(sp)
            sd   a0, 1*8(sp)
            sd   a1, 2*8(sp)
        ",
        // 清除 msip 设置 ssip
        "   la   a0, {clint_ptr}
            ld   a0, (a0)
            csrr a1, mhartid
            call {clear_msip}
            csrrsi zero, mip, 1 << 1
        ",
        // 恢复
        "   ld   ra, 0*8(sp)
            ld   a0, 1*8(sp)
            ld   a1, 2*8(sp)
            addi sp, sp,  3*8
        ",
        // 换栈：
        // sp      : S sp
        // mscratch: M sp
        "   csrrw sp, mscratch, sp",
        // 返回
        "   mret",
        clint_ptr  = sym SIFIVECLINT,
        //               Clint::clear_msip_naked(&self, hart_idx)
        clear_msip = sym Clint::clear_msip_naked,
        options(noreturn)
    )
}

/// Fast Trap
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
    use riscv::register::{
        mcause::{self, Exception as E, Trap as T},
        mtval, satp, sstatus,
    };

    #[inline]
    fn boot(mut ctx: FastContext, start_addr: usize, opaque: usize) -> FastResult {
        unsafe {
            sstatus::clear_sie();
            satp::write(0);
        }
        ctx.regs().a[0] = hart_id();
        ctx.regs().a[1] = opaque;
        ctx.regs().pc = start_addr;
        ctx.call(2)
    }
    loop {
        match local_hsm().start() {
            Ok(next_stage) => {
                unsafe {
                    mstatus::set_mpie();
                    mstatus::set_mpp(next_stage.next_mode);
                    mie::set_msoft();
                    mie::set_mtimer();
                }
                break boot(ctx, next_stage.start_addr, next_stage.opaque);
            }
            Err(rustsbi::spec::hsm::HART_STOP) => {
                unsafe {
                    mie::set_msoft();
                }
                riscv::asm::wfi();
                clint::clear_msip();
            }
            _ => match mcause::read().cause() {
                // SBI call
                T::Exception(E::SupervisorEnvCall) => {
                    use sbi_spec::{base, hsm, legacy};
                    let mut ret = unsafe { SBI.assume_init_mut() }.handle_ecall(
                        a7,
                        a6,
                        [ctx.a0(), a1, a2, a3, a4, a5],
                    );
                    if ret.is_ok() {
                        match (a7, a6) {
                            // 关闭
                            (hsm::EID_HSM, hsm::HART_STOP) => continue,
                            // 不可恢复挂起
                            (hsm::EID_HSM, hsm::HART_SUSPEND)
                                if matches!(ctx.a0() as u32, hsm::suspend_type::NON_RETENTIVE) =>
                            {
                                break boot(ctx, a1, a2);
                            }
                            // legacy console 探测
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
                                print!("{}", ctx.a0() as u8 as char);
                                ret.error = 0;
                                ret.value = a1;
                            }
                            legacy::LEGACY_CONSOLE_GETCHAR => {
                                let mut c = 0u8;
                                let uart = CONSOLE.lock();
                                match *uart {
                                    MachineConsole::Uart16550(uart16550) => unsafe {
                                        loop {
                                            if (*uart16550).read(core::slice::from_mut(&mut c)) == 1
                                            {
                                                ret.error = c as _;
                                                ret.value = a1;
                                                break;
                                            }
                                        }
                                    },
                                }
                                drop(uart);
                            }
                            _ => {}
                        }
                    }
                    ctx.regs().a = [ret.error, ret.value, a2, a3, a4, a5, a6, a7];
                    mepc::write(mepc::read() + 4);
                    break ctx.restore();
                }
                T::Exception(E::IllegalInstruction) => {
                    if mstatus::read().mpp() == mstatus::MPP::Machine {
                        panic!("Cannot handle illegal instruction exception from M-MODE");
                    }

                    ctx.regs().a = [ctx.a0(), a1, a2, a3, a4, a5, a6, a7];
                    if !illegal_instruction_handler(&mut ctx) {
                        delegate();
                    }
                    break ctx.restore();
                }
                // 其他陷入
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
                    panic!("stopped with unsupported trap")
                }
            },
        }
    }
}

#[inline]
fn delegate() {
    use riscv::register::{sepc, scause, stval, stvec, sstatus, mepc, mcause, mtval};
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
        mepc::write(stvec::read().address());
    }
}

#[inline]
fn illegal_instruction_handler(ctx: &mut FastContext) -> bool {
    use riscv_decode::{decode, Instruction};
    use riscv::register::{mepc, mtval};

    let inst = decode(mtval::read() as u32);
    match inst {
        Ok(Instruction::Csrrs(csr)) => match csr.csr() {
            CSR_TIME => {
                assert!(
                    10 <= csr.rd() && csr.rd() <= 17,
                    "Unsupported CSR rd: {}",
                    csr.rd()
                );
                ctx.regs().a[(csr.rd() - 10) as usize] = unsafe { SBI.assume_init_mut() }.clint.as_ref().unwrap().get_time();
            }
            CSR_TIMEH => {
                assert!(
                    10 <= csr.rd() && csr.rd() <= 17,
                    "Unsupported CSR rd: {}",
                    csr.rd()
                );
                ctx.regs().a[(csr.rd() - 10) as usize] = unsafe { SBI.assume_init_mut() }.clint.as_ref().unwrap().get_timeh();
            }
            _ => return false,
        },
        _ => return false,
    }
    mepc::write(mepc::read() + 4);
    true
}
