use core::arch::asm;
use core::arch::naked_asm;
use riscv::register::mtvec;

/// When you expected some insts will cause trap, use this.
/// If trap happened, a0 will set to 1, otherwise will be 0.
///
/// This function will change a0 and a1 and will NOT change them back.
// TODO: Support save trap info.
#[naked]
#[repr(align(16))]
pub(crate) unsafe extern "C" fn light_expected_trap() {
    unsafe {
        naked_asm!(
            "add a0, zero, zero",
            "add a1, zero, zero",
            "csrr a1, mepc",
            "addi a1, a1, 4",
            "csrw mepc, a1",
            "addi a0, zero, 1",
            "mret",
        )
    }
}

#[repr(C)]
pub struct TrapInfo {
    pub mepc: usize,
    pub mcause: usize,
    pub mtval: usize,
}

impl Default for TrapInfo {
    fn default() -> Self {
        Self {
            mepc: 0,
            mcause: 0,
            mtval: 0,
        }
    }
}

#[naked]
#[repr(align(16))]
pub(crate) unsafe extern "C" fn expected_trap() {
    unsafe {
        naked_asm!(
            "csrr a4, mepc",
            "sd a4, 0*8(a3)",
            "csrr a4, mcause",
            "sd a4, 1*8(a3)",
            "csrr a4, mtval",
            "sd a4, 2*8(a3)",
            "csrr a4, mepc",
            "addi a4, a4, 4",
            "csrw mepc, a4",
            "mret",
        )
    }
}

pub(crate) unsafe fn csr_read_allow<const CSR_NUM: u32>(trap_info: *mut TrapInfo) -> usize {
    let tinfo = trap_info as usize;
    let mut ret: usize;
    // Backup old mtvec
    let mtvec = mtvec::read().bits();
    unsafe {
        core::ptr::write_volatile(&mut (*trap_info).mcause, usize::MAX);
        // Write expected_trap
        mtvec::write(expected_trap as _, mtvec::TrapMode::Direct);

        asm!(
            "add a3, {tinfo}, zero",
            "add a4, {tinfo}, zero",
            "csrr {ret}, {csr}",
            tinfo = in(reg) tinfo,
            ret = out(reg) ret,
            csr = const CSR_NUM,
            options(nostack, preserves_flags)
        );
        asm!("csrw mtvec, {}", in(reg) mtvec);
    }
    ret
}

pub(crate) unsafe fn csr_write_allow<const CSR_NUM: u32>(trap_info: *mut TrapInfo, value: usize) {
    let tinfo = trap_info as usize;
    // Backup old mtvec
    let mtvec = mtvec::read().bits();
    unsafe {
        core::ptr::write_volatile(&mut (*trap_info).mcause, usize::MAX);
        // Write expected_trap
        mtvec::write(expected_trap as _, mtvec::TrapMode::Direct);

        asm!(
            "add a3, {tinfo}, zero",
            "add a4, {tinfo}, zero",
            "csrw {csr}, {value}",
            tinfo = in(reg) tinfo,
            csr = const CSR_NUM,
            value = in(reg) value,
            options(nostack, preserves_flags)
        );
        asm!("csrw mtvec, {}", in(reg) mtvec);
    }
}

pub(crate) unsafe fn csr_swap<const CSR_NUM: u32>(val: usize) -> usize {
    let ret: usize;

    unsafe {
        asm!(
            "csrrw {ret}, {csr}, {val}",
            csr = const CSR_NUM,
            val = in(reg) val,
            ret = out(reg) ret,
            options(nostack, preserves_flags)
        );
    }
    ret
}
