#![allow(unused)]

/// CSR addresses
pub const CSR_STIMECMP: u16 = 0x14D;
pub const CSR_MCOUNTEREN: u16 = 0x306;
pub const CSR_MENVCFG: u16 = 0x30a;
pub const CSR_MCYCLE: u16 = 0xb00;
pub const CSR_MINSTRET: u16 = 0xb02;
pub const CSR_MHPMCOUNTER3: u16 = 0xb03;
pub const CSR_MHPMCOUNTER4: u16 = 0xb04;
pub const CSR_MHPMCOUNTER5: u16 = 0xb05;
pub const CSR_MHPMCOUNTER6: u16 = 0xb06;
pub const CSR_MHPMCOUNTER7: u16 = 0xb07;
pub const CSR_MHPMCOUNTER8: u16 = 0xb08;
pub const CSR_MHPMCOUNTER9: u16 = 0xb09;
pub const CSR_MHPMCOUNTER10: u16 = 0xb0a;
pub const CSR_MHPMCOUNTER11: u16 = 0xb0b;
pub const CSR_MHPMCOUNTER12: u16 = 0xb0c;
pub const CSR_MHPMCOUNTER13: u16 = 0xb0d;
pub const CSR_MHPMCOUNTER14: u16 = 0xb0e;
pub const CSR_MHPMCOUNTER15: u16 = 0xb0f;
pub const CSR_MHPMCOUNTER16: u16 = 0xb10;
pub const CSR_MHPMCOUNTER17: u16 = 0xb11;
pub const CSR_MHPMCOUNTER18: u16 = 0xb12;
pub const CSR_MHPMCOUNTER19: u16 = 0xb13;
pub const CSR_MHPMCOUNTER20: u16 = 0xb14;
pub const CSR_MHPMCOUNTER21: u16 = 0xb15;
pub const CSR_MHPMCOUNTER22: u16 = 0xb16;
pub const CSR_MHPMCOUNTER23: u16 = 0xb17;
pub const CSR_MHPMCOUNTER24: u16 = 0xb18;
pub const CSR_MHPMCOUNTER25: u16 = 0xb19;
pub const CSR_MHPMCOUNTER26: u16 = 0xb1a;
pub const CSR_MHPMCOUNTER27: u16 = 0xb1b;
pub const CSR_MHPMCOUNTER28: u16 = 0xb1c;
pub const CSR_MHPMCOUNTER29: u16 = 0xb1d;
pub const CSR_MHPMCOUNTER30: u16 = 0xb1e;
pub const CSR_MHPMCOUNTER31: u16 = 0xb1f;
pub const CSR_MCYCLEH: u16 = 0xb80;
pub const CSR_MINSTRETH: u16 = 0xb82;
pub const CSR_MHPMCOUNTER3H: u16 = 0xb83;
pub const CSR_MHPMCOUNTER4H: u16 = 0xb84;
pub const CSR_MHPMCOUNTER5H: u16 = 0xb85;
pub const CSR_MHPMCOUNTER6H: u16 = 0xb86;
pub const CSR_MHPMCOUNTER7H: u16 = 0xb87;
pub const CSR_MHPMCOUNTER8H: u16 = 0xb88;
pub const CSR_MHPMCOUNTER9H: u16 = 0xb89;
pub const CSR_MHPMCOUNTER10H: u16 = 0xb8a;
pub const CSR_MHPMCOUNTER11H: u16 = 0xb8b;
pub const CSR_MHPMCOUNTER12H: u16 = 0xb8c;
pub const CSR_MHPMCOUNTER13H: u16 = 0xb8d;
pub const CSR_MHPMCOUNTER14H: u16 = 0xb8e;
pub const CSR_MHPMCOUNTER15H: u16 = 0xb8f;
pub const CSR_MHPMCOUNTER16H: u16 = 0xb90;
pub const CSR_MHPMCOUNTER17H: u16 = 0xb91;
pub const CSR_MHPMCOUNTER18H: u16 = 0xb92;
pub const CSR_MHPMCOUNTER19H: u16 = 0xb93;
pub const CSR_MHPMCOUNTER20H: u16 = 0xb94;
pub const CSR_MHPMCOUNTER21H: u16 = 0xb95;
pub const CSR_MHPMCOUNTER22H: u16 = 0xb96;
pub const CSR_MHPMCOUNTER23H: u16 = 0xb97;
pub const CSR_MHPMCOUNTER24H: u16 = 0xb98;
pub const CSR_MHPMCOUNTER25H: u16 = 0xb99;
pub const CSR_MHPMCOUNTER26H: u16 = 0xb9a;
pub const CSR_MHPMCOUNTER27H: u16 = 0xb9b;
pub const CSR_MHPMCOUNTER28H: u16 = 0xb9c;
pub const CSR_MHPMCOUNTER29H: u16 = 0xb9d;
pub const CSR_MHPMCOUNTER30H: u16 = 0xb9e;
pub const CSR_MHPMCOUNTER31H: u16 = 0xb9f;
/* User Counters/Timers */
pub const CSR_CYCLE: u16 = 0xc00;
pub const CSR_TIME: u16 = 0xc01;
pub const CSR_INSTRET: u16 = 0xc02;
pub const CSR_HPMCOUNTER3: u16 = 0xc03;
pub const CSR_HPMCOUNTER4: u16 = 0xc04;
pub const CSR_HPMCOUNTER5: u16 = 0xc05;
pub const CSR_HPMCOUNTER6: u16 = 0xc06;
pub const CSR_HPMCOUNTER7: u16 = 0xc07;
pub const CSR_HPMCOUNTER8: u16 = 0xc08;
pub const CSR_HPMCOUNTER9: u16 = 0xc09;
pub const CSR_HPMCOUNTER10: u16 = 0xc0a;
pub const CSR_HPMCOUNTER11: u16 = 0xc0b;
pub const CSR_HPMCOUNTER12: u16 = 0xc0c;
pub const CSR_HPMCOUNTER13: u16 = 0xc0d;
pub const CSR_HPMCOUNTER14: u16 = 0xc0e;
pub const CSR_HPMCOUNTER15: u16 = 0xc0f;
pub const CSR_HPMCOUNTER16: u16 = 0xc10;
pub const CSR_HPMCOUNTER17: u16 = 0xc11;
pub const CSR_HPMCOUNTER18: u16 = 0xc12;
pub const CSR_HPMCOUNTER19: u16 = 0xc13;
pub const CSR_HPMCOUNTER20: u16 = 0xc14;
pub const CSR_HPMCOUNTER21: u16 = 0xc15;
pub const CSR_HPMCOUNTER22: u16 = 0xc16;
pub const CSR_HPMCOUNTER23: u16 = 0xc17;
pub const CSR_HPMCOUNTER24: u16 = 0xc18;
pub const CSR_HPMCOUNTER25: u16 = 0xc19;
pub const CSR_HPMCOUNTER26: u16 = 0xc1a;
pub const CSR_HPMCOUNTER27: u16 = 0xc1b;
pub const CSR_HPMCOUNTER28: u16 = 0xc1c;
pub const CSR_HPMCOUNTER29: u16 = 0xc1d;
pub const CSR_HPMCOUNTER30: u16 = 0xc1e;
pub const CSR_HPMCOUNTER31: u16 = 0xc1f;
/// MHPMEVENT
pub const CSR_MCOUNTINHIBIT: u16 = 0x320;
pub const CSR_MCYCLECFG: u16 = 0x321;
pub const CSR_MINSTRETCFG: u16 = 0x322;
pub const CSR_MHPMEVENT3: u16 = 0x323;
pub const CSR_MHPMEVENT4: u16 = 0x324;
pub const CSR_MHPMEVENT5: u16 = 0x325;
pub const CSR_MHPMEVENT6: u16 = 0x326;
pub const CSR_MHPMEVENT7: u16 = 0x327;
pub const CSR_MHPMEVENT8: u16 = 0x328;
pub const CSR_MHPMEVENT9: u16 = 0x329;
pub const CSR_MHPMEVENT10: u16 = 0x32a;
pub const CSR_MHPMEVENT11: u16 = 0x32b;
pub const CSR_MHPMEVENT12: u16 = 0x32c;
pub const CSR_MHPMEVENT13: u16 = 0x32d;
pub const CSR_MHPMEVENT14: u16 = 0x32e;
pub const CSR_MHPMEVENT15: u16 = 0x32f;
pub const CSR_MHPMEVENT16: u16 = 0x330;
pub const CSR_MHPMEVENT17: u16 = 0x331;
pub const CSR_MHPMEVENT18: u16 = 0x332;
pub const CSR_MHPMEVENT19: u16 = 0x333;
pub const CSR_MHPMEVENT20: u16 = 0x334;
pub const CSR_MHPMEVENT21: u16 = 0x335;
pub const CSR_MHPMEVENT22: u16 = 0x336;
pub const CSR_MHPMEVENT23: u16 = 0x337;
pub const CSR_MHPMEVENT24: u16 = 0x338;
pub const CSR_MHPMEVENT25: u16 = 0x339;
pub const CSR_MHPMEVENT26: u16 = 0x33a;
pub const CSR_MHPMEVENT27: u16 = 0x33b;
pub const CSR_MHPMEVENT28: u16 = 0x33c;
pub const CSR_MHPMEVENT29: u16 = 0x33d;
pub const CSR_MHPMEVENT30: u16 = 0x33e;
pub const CSR_MHPMEVENT31: u16 = 0x33f;

// For RV32
pub const CSR_CYCLEH: u16 = 0xc80;
pub const CSR_TIMEH: u16 = 0xc81;
pub const CSR_INSTRETH: u16 = 0xc82;
pub const CSR_HPMCOUNTER3H: u16 = 0xc83;
pub const CSR_HPMCOUNTER4H: u16 = 0xc84;
pub const CSR_HPMCOUNTER5H: u16 = 0xc85;
pub const CSR_HPMCOUNTER6H: u16 = 0xc86;
pub const CSR_HPMCOUNTER7H: u16 = 0xc87;
pub const CSR_HPMCOUNTER8H: u16 = 0xc88;
pub const CSR_HPMCOUNTER9H: u16 = 0xc89;
pub const CSR_HPMCOUNTER10H: u16 = 0xc8a;
pub const CSR_HPMCOUNTER11H: u16 = 0xc8b;
pub const CSR_HPMCOUNTER12H: u16 = 0xc8c;
pub const CSR_HPMCOUNTER13H: u16 = 0xc8d;
pub const CSR_HPMCOUNTER14H: u16 = 0xc8e;
pub const CSR_HPMCOUNTER15H: u16 = 0xc8f;
pub const CSR_HPMCOUNTER16H: u16 = 0xc90;
pub const CSR_HPMCOUNTER17H: u16 = 0xc91;
pub const CSR_HPMCOUNTER18H: u16 = 0xc92;
pub const CSR_HPMCOUNTER19H: u16 = 0xc93;
pub const CSR_HPMCOUNTER20H: u16 = 0xc94;
pub const CSR_HPMCOUNTER21H: u16 = 0xc95;
pub const CSR_HPMCOUNTER22H: u16 = 0xc96;
pub const CSR_HPMCOUNTER23H: u16 = 0xc97;
pub const CSR_HPMCOUNTER24H: u16 = 0xc98;
pub const CSR_HPMCOUNTER25H: u16 = 0xc99;
pub const CSR_HPMCOUNTER26H: u16 = 0xc9a;
pub const CSR_HPMCOUNTER27H: u16 = 0xc9b;
pub const CSR_HPMCOUNTER28H: u16 = 0xc9c;
pub const CSR_HPMCOUNTER29H: u16 = 0xc9d;
pub const CSR_HPMCOUNTER30H: u16 = 0xc9e;
pub const CSR_HPMCOUNTER31H: u16 = 0xc9f;

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
