#![allow(unused)]

/// CSR addresses for timer registers.
///
/// Time value (lower 32 bits).
pub const CSR_TIME: u32 = 0xc01;
/// Time value (upper 32 bits).
pub const CSR_TIMEH: u32 = 0xc81;
/// Supervisor timer compare value.
pub const CSR_STIMECMP: u32 = 0x14D;

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
