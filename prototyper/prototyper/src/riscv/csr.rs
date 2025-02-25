#![allow(unused)]

/// CSR addresses for timer registers.
///
/// Time value (lower 32 bits).
pub const CSR_TIME: u32 = 0xc01;
/// Time value (upper 32 bits).
pub const CSR_TIMEH: u32 = 0xc81;
/// Supervisor timer compare value.
pub const CSR_STIMECMP: u32 = 0x14D;
pub const CSR_MCOUNTEREN: u32 = 0x306;
pub const CSR_MCOUNTINHIBIT: u32 = 0x320;
pub const CSR_MENVCFG: u32 = 0x30a;
/* Machine Counters/Timers */
pub const CSR_MCYCLE: u32 = 0xb00;
pub const CSR_MINSTRET: u32 = 0xb02;
pub const CSR_MHPMCOUNTER3: u32 = 0xb03;
pub const CSR_MHPMCOUNTER4: u32 = 0xb04;
pub const CSR_MHPMCOUNTER5: u32 = 0xb05;
pub const CSR_MHPMCOUNTER6: u32 = 0xb06;
pub const CSR_MHPMCOUNTER7: u32 = 0xb07;
pub const CSR_MHPMCOUNTER8: u32 = 0xb08;
pub const CSR_MHPMCOUNTER9: u32 = 0xb09;
pub const CSR_MHPMCOUNTER10: u32 = 0xb0a;
pub const CSR_MHPMCOUNTER11: u32 = 0xb0b;
pub const CSR_MHPMCOUNTER12: u32 = 0xb0c;
pub const CSR_MHPMCOUNTER13: u32 = 0xb0d;
pub const CSR_MHPMCOUNTER14: u32 = 0xb0e;
pub const CSR_MHPMCOUNTER15: u32 = 0xb0f;
pub const CSR_MHPMCOUNTER16: u32 = 0xb10;
pub const CSR_MHPMCOUNTER17: u32 = 0xb11;
pub const CSR_MHPMCOUNTER18: u32 = 0xb12;
pub const CSR_MHPMCOUNTER19: u32 = 0xb13;
pub const CSR_MHPMCOUNTER20: u32 = 0xb14;
pub const CSR_MHPMCOUNTER21: u32 = 0xb15;
pub const CSR_MHPMCOUNTER22: u32 = 0xb16;
pub const CSR_MHPMCOUNTER23: u32 = 0xb17;
pub const CSR_MHPMCOUNTER24: u32 = 0xb18;
pub const CSR_MHPMCOUNTER25: u32 = 0xb19;
pub const CSR_MHPMCOUNTER26: u32 = 0xb1a;
pub const CSR_MHPMCOUNTER27: u32 = 0xb1b;
pub const CSR_MHPMCOUNTER28: u32 = 0xb1c;
pub const CSR_MHPMCOUNTER29: u32 = 0xb1d;
pub const CSR_MHPMCOUNTER30: u32 = 0xb1e;
pub const CSR_MHPMCOUNTER31: u32 = 0xb1f;
pub const CSR_MCYCLEH: u32 = 0xb80;
pub const CSR_MINSTRETH: u32 = 0xb82;
pub const CSR_MHPMCOUNTER3H: u32 = 0xb83;
pub const CSR_MHPMCOUNTER4H: u32 = 0xb84;
pub const CSR_MHPMCOUNTER5H: u32 = 0xb85;
pub const CSR_MHPMCOUNTER6H: u32 = 0xb86;
pub const CSR_MHPMCOUNTER7H: u32 = 0xb87;
pub const CSR_MHPMCOUNTER8H: u32 = 0xb88;
pub const CSR_MHPMCOUNTER9H: u32 = 0xb89;
pub const CSR_MHPMCOUNTER10H: u32 = 0xb8a;
pub const CSR_MHPMCOUNTER11H: u32 = 0xb8b;
pub const CSR_MHPMCOUNTER12H: u32 = 0xb8c;
pub const CSR_MHPMCOUNTER13H: u32 = 0xb8d;
pub const CSR_MHPMCOUNTER14H: u32 = 0xb8e;
pub const CSR_MHPMCOUNTER15H: u32 = 0xb8f;
pub const CSR_MHPMCOUNTER16H: u32 = 0xb90;
pub const CSR_MHPMCOUNTER17H: u32 = 0xb91;
pub const CSR_MHPMCOUNTER18H: u32 = 0xb92;
pub const CSR_MHPMCOUNTER19H: u32 = 0xb93;
pub const CSR_MHPMCOUNTER20H: u32 = 0xb94;
pub const CSR_MHPMCOUNTER21H: u32 = 0xb95;
pub const CSR_MHPMCOUNTER22H: u32 = 0xb96;
pub const CSR_MHPMCOUNTER23H: u32 = 0xb97;
pub const CSR_MHPMCOUNTER24H: u32 = 0xb98;
pub const CSR_MHPMCOUNTER25H: u32 = 0xb99;
pub const CSR_MHPMCOUNTER26H: u32 = 0xb9a;
pub const CSR_MHPMCOUNTER27H: u32 = 0xb9b;
pub const CSR_MHPMCOUNTER28H: u32 = 0xb9c;
pub const CSR_MHPMCOUNTER29H: u32 = 0xb9d;
pub const CSR_MHPMCOUNTER30H: u32 = 0xb9e;
pub const CSR_MHPMCOUNTER31H: u32 = 0xb9f;

/// Machine environment configuration register (menvcfg) bit fields.
pub mod menvcfg {
    use core::arch::asm;

    /// Fence of I/O implies memory.
    pub const FIOM: usize = 0x1 << 0;
    /// Cache block invalidate - flush.
    pub const CBIE_FLUSH: usize = 0x01 << 4;
    /// Cache block invalidate - invalidate.
    pub const CBIE_INVALIDATE: usize = 0x11 << 4;
    /// Cache block clean for enclave.
    pub const CBCFE: usize = 0x1 << 6;
    /// Cache block zero for enclave.
    pub const CBZE: usize = 0x1 << 7;
    /// Page-based memory types enable.
    pub const PBMTE: usize = 0x1 << 62;
    /// Supervisor timer counter enable.
    pub const STCE: usize = 0x1 << 63;

    /// Sets the STCE bit to enable supervisor timer counter.
    #[inline(always)]
    pub fn set_stce() {
        set_bits(STCE);
    }

    /// Sets specified bits in menvcfg register.
    pub fn set_bits(option: usize) {
        let mut bits: usize;
        unsafe {
            // Read current `menvcfg` value.
            asm!("csrr {}, menvcfg", out(reg) bits, options(nomem));
        }
        // Set requested bits
        bits |= option;
        unsafe {
            // Write back updated value
            asm!("csrw menvcfg, {}", in(reg) bits, options(nomem));
        }
    }
}

/// Supervisor timer compare register operations.
pub mod stimecmp {
    use core::arch::asm;

    /// Sets the supervisor timer compare value.
    pub fn set(value: u64) {
        unsafe {
            asm!("csrrw zero, stimecmp, {}", in(reg) value, options(nomem));
        }
    }
}
