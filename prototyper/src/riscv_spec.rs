#![allow(unused)]

pub const CSR_TIME: u32 = 0xc01;
pub const CSR_TIMEH: u32 = 0xc81;
pub const CSR_STIMECMP: u32 = 0x14D;

pub mod menvcfg {
    use core::arch::asm;

    pub const FIOM: usize = 0x1 << 0;
    pub const CBIE_FLUSH: usize = 0x01 << 4;
    pub const CBIE_INVALIDATE: usize = 0x11 << 4;
    pub const CBCFE: usize = 0x1 << 6;
    pub const CBZE: usize = 0x1 << 7;
    pub const PBMTE: usize = 0x1 << 62;
    pub const STCE: usize = 0x1 << 63;

    #[inline(always)]
    pub fn set_stce() {
        set_bits(STCE);
    }

    pub fn set_bits(option: usize) {
        let mut bits: usize;
        unsafe { asm!("csrr {}, menvcfg", out(reg) bits, options(nomem)); }
        bits |= option;
        unsafe { asm!("csrw menvcfg, {}", in(reg) bits, options(nomem)); }
    }
}

pub mod stimecmp {
    use core::arch::asm;

    pub fn set(value: u64) {
        unsafe { asm!("csrrs zero, stimecmp, {}", in(reg) value, options(nomem)); }
    }
}


#[inline]
pub fn current_hartid() -> usize {
    riscv::register::mhartid::read()
}
