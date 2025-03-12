use core::arch::asm;
use riscv::register::{mtvec, sscratch};

use fast_trap::EntireContextSeparated;

const MPRV_BIT: usize = 1usize << 17;
const MXR_BIT: usize = 1usize << 19;

pub enum VarType {
    Signed,
    UnSigned,
    Float,
}

#[inline(always)]
pub fn read_gp() -> usize {
    let mut data: usize;
    unsafe { asm!("mv {}, gp", out(reg) data, options(nomem)) };
    data
}

#[inline(always)]
pub fn read_tp() -> usize {
    let mut data: usize;
    unsafe { asm!("mv {}, tp", out(reg) data, options(nomem)) };
    data
}

#[inline(always)]
pub fn write_gp(data: usize) {
    unsafe { asm!("mv gp, {}", in(reg) data, options(nomem)) };
}

#[inline(always)]
pub fn write_tp(data: usize) {
    unsafe { asm!("mv tp, {}", in(reg) data, options(nomem)) };
}

// If inline this and next function will cause crash. It looks like magic.
#[inline(never)]
pub fn get_unsigned_byte(addr: usize) -> u8 {
    let mut data: usize = 0;
    let mut status: usize = 0;
    unsafe {
        let prev_mtvec = mtvec::read().bits();
        mtvec::write(
            crate::sbi::early_trap::expected_trap as _,
            mtvec::TrapMode::Direct,
        );
        asm!(
            "csrrs t3, mstatus, t3",
            "lbu t0, 0(t1)",
            "csrw mstatus, t3",
            "csrw mtvec, t4",
            in("t1") addr,
            in("t3") MPRV_BIT | MXR_BIT,
            in("a1") 0,
            in("t4") prev_mtvec,
            inout("t0") data,
            inout("a0") status,
        );
        if status != 0 {
            panic!("Error when load byte");
        }
    }
    data as u8
}

#[inline(never)]
pub fn save_byte(addr: usize, data: usize) {
    unsafe {
        let prev_mtvec = mtvec::read().bits();
        let mut status: usize = 0;
        mtvec::write(
            crate::sbi::early_trap::expected_trap as _,
            mtvec::TrapMode::Direct,
        );
        asm!(
              "csrrs t3, mstatus, t3",
              "sb t0, 0(t1)",
              "csrw mstatus, t3",
              "csrw mtvec, t4",
            in("t0") data,
            in("t1") addr,
            in("t4") prev_mtvec,
            in("a1") 0,
            in("t3") MPRV_BIT | MXR_BIT,
            inout("a0") status,
        );
        if status != 0 {
            panic!("Error when save byte");
        }
    }
}

#[inline(always)]
pub fn get_data(addr: usize, len: usize) -> usize {
    let mut data: usize = 0;
    for i in (addr..addr + len).rev() {
        data <<= 8;
        data |= get_unsigned_byte(i) as usize;
    }
    data
}

#[inline(always)]
pub fn get_inst(addr: usize) -> (usize, usize) {
    let low_data = get_data(addr, 2);
    // We assume we only have 16bit and 32bit inst.
    if riscv_decode::instruction_length(low_data as u16) == 2 {
        return (low_data, 2);
    } else {
        return (low_data | (get_data(addr + 2, 2) << 16), 4);
    }
}

#[inline(always)]
pub fn save_reg_x(ctx: &mut EntireContextSeparated, reg_id: usize, data: usize) {
    match reg_id {
        00 => (),
        01 => ctx.regs().ra = data,
        02 => sscratch::write(data),
        03 => write_gp(data),
        04 => write_tp(data),
        05 => ctx.regs().t[0] = data,
        06 => ctx.regs().t[1] = data,
        07 => ctx.regs().t[2] = data,
        08 => ctx.regs().s[0] = data,
        09 => ctx.regs().s[1] = data,
        // x10 = a0 ..= x17 = a7
        10..=17 => ctx.regs().a[reg_id - 10] = data,
        // x18 = s2 ..= x27 = s11
        18..=27 => ctx.regs().s[reg_id - 16] = data,
        // x28 = t3 ..= x31 = t6
        28..=31 => ctx.regs().t[reg_id - 25] = data,
        _ => panic!(),
    }
}

#[inline(always)]
pub fn get_reg_x(ctx: &mut EntireContextSeparated, reg_id: usize) -> usize {
    match reg_id {
        00 => 0,
        01 => ctx.regs().ra,
        02 => sscratch::read(),
        03 => read_gp(),
        04 => read_tp(),
        05 => ctx.regs().t[0],
        06 => ctx.regs().t[1],
        07 => ctx.regs().t[2],
        08 => ctx.regs().s[0],
        09 => ctx.regs().s[1],
        // x10 = a0 ..= x17 = a7
        10..=17 => ctx.regs().a[reg_id - 10],
        // x18 = s2 ..= x27 = s11
        18..=27 => ctx.regs().s[reg_id - 16],
        // x28 = t3 ..= x31 = t6
        28..=31 => ctx.regs().t[reg_id - 25],
        _ => panic!(),
    }
}
