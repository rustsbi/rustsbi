#![allow(unused)]

use pastey::paste;
use seq_macro::seq;

/// CSR addresses
pub const CSR_STIMECMP: u16 = 0x14D;
pub const CSR_MCOUNTEREN: u16 = 0x306;
pub const CSR_MENVCFG: u16 = 0x30a;
pub const CSR_MCYCLE: u16 = 0xb00;
pub const CSR_MINSTRET: u16 = 0xb02;
seq!(N in 3..32 {
    pub const CSR_MHPMCOUNTER~N: u16 = 0xb00 + N;
});
pub const CSR_MCYCLEH: u16 = 0xb80;
pub const CSR_MINSTRETH: u16 = 0xb82;
seq!(N in 3..32 {
    paste! {
        pub const [<CSR_MHPMCOUNTER ~N H>]: u16 = 0xb80 + N;
    }
});
/* User Counters/Timers */
pub const CSR_CYCLE: u16 = 0xc00;
pub const CSR_TIME: u16 = 0xc01;
pub const CSR_INSTRET: u16 = 0xc02;
seq!(N in 3..32 {
    pub const CSR_HPMCOUNTER~N: u16 = 0xc00 + N;
});
/// MHPMEVENT
pub const CSR_MCOUNTINHIBIT: u16 = 0x320;
pub const CSR_MCYCLECFG: u16 = 0x321;
pub const CSR_MINSTRETCFG: u16 = 0x322;
seq!(N in 3..32 {
    pub const CSR_MHPMEVENT~N: u16 = 0x320 + N;
});

// For RV32
pub const CSR_CYCLEH: u16 = 0xc80;
pub const CSR_TIMEH: u16 = 0xc81;
pub const CSR_INSTRETH: u16 = 0xc82;
seq!(N in 3..32 {
    paste!{ pub const [<CSR_HPMCOUNTER ~N H>]: u16 = 0xc80 + N; }
});

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

pub mod mcycle {
    use core::arch::asm;
    pub fn write(value: u64) {
        unsafe {
            asm!("csrrw zero, mcycle, {}", in(reg) value, options(nomem));
        }
    }
}

pub mod minstret {
    use core::arch::asm;
    pub fn write(value: u64) {
        unsafe {
            asm!("csrrw zero, minstret, {}", in(reg) value, options(nomem));
        }
    }
}
