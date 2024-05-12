use core::arch::asm;

use riscv::register::{
    mstatus::Mstatus,
    mtvec::{self, TrapMode},
};

// Typically, the `stvec` Will be overrided by supervisor software, e.g. kernel, hypervisor or secure environments.
pub fn init() {
    let mut addr = machine_trap as usize;
    if addr & 0b10 != 0 {
        addr += 0b10; // mtvec base address must align to 4 bytes.
    }
    unsafe { mtvec::write(addr, TrapMode::Direct) };
}

extern "C" fn rust_machine_trap(ctx: &mut SupervisorContext) {
    error!("ctx: {:x?}", ctx);
}

#[naked]
unsafe extern "C" fn machine_trap() -> ! {
    asm!(
        ".align 2",
        "csrrw  sp, mscratch, sp",
        "sd     ra, 0*8(sp)
        sd      gp, 2*8(sp)
        sd      tp, 3*8(sp)
        sd      t0, 4*8(sp)
        sd      t1, 5*8(sp)
        sd      t2, 6*8(sp)
        sd      s0, 7*8(sp)
        sd      s1, 8*8(sp)
        sd      a0, 9*8(sp)
        sd      a1, 10*8(sp)
        sd      a2, 11*8(sp)
        sd      a3, 12*8(sp)
        sd      a4, 13*8(sp)
        sd      a5, 14*8(sp)
        sd      a6, 15*8(sp)
        sd      a7, 16*8(sp)
        sd      s2, 17*8(sp)
        sd      s3, 18*8(sp)
        sd      s4, 19*8(sp)
        sd      s5, 20*8(sp)
        sd      s6, 21*8(sp)
        sd      s7, 22*8(sp)
        sd      s8, 23*8(sp)
        sd      s9, 24*8(sp)
        sd     s10, 25*8(sp)
        sd     s11, 26*8(sp)
        sd      t3, 27*8(sp)
        sd      t4, 28*8(sp)
        sd      t5, 29*8(sp)
        sd      t6, 30*8(sp)",
        "csrr   t0, mstatus
        sd      t0, 31*8(sp)",
        "csrr   t1, mepc
        sd      t1, 32*8(sp)",
        "csrr   t2, mscratch",
        "sd     t2, 1*8(sp)",
        "mv     a0, sp",
        "call   {rust_machine_trap}",
        "ld     t0, 31*8(sp)
        ld      t1, 32*8(sp)
        csrw    mstatus, t0
        csrw    mepc, t1",
        "ld     ra, 0*8(sp)
        ld      gp, 2*8(sp)
        ld      tp, 3*8(sp)
        ld      t0, 4*8(sp)
        ld      t1, 5*8(sp)
        ld      t2, 6*8(sp)
        ld      s0, 7*8(sp)
        ld      s1, 8*8(sp)
        ld      a0, 9*8(sp)
        ld      a1, 10*8(sp)
        ld      a2, 11*8(sp)
        ld      a3, 12*8(sp)
        ld      a4, 13*8(sp)
        ld      a5, 14*8(sp)
        ld      a6, 15*8(sp)
        ld      a7, 16*8(sp)
        ld      s2, 17*8(sp)
        ld      s3, 18*8(sp)
        ld      s4, 19*8(sp)
        ld      s5, 20*8(sp)
        ld      s6, 21*8(sp)
        ld      s7, 22*8(sp)
        ld      s8, 23*8(sp)
        ld      s9, 24*8(sp)
        ld     s10, 25*8(sp)
        ld     s11, 26*8(sp)
        ld      t3, 27*8(sp)
        ld      t4, 28*8(sp)
        ld      t5, 29*8(sp)
        ld      t6, 30*8(sp)",
        "csrrw  sp, mscratch, sp",
        "mret",
        rust_machine_trap = sym rust_machine_trap,
        options(noreturn)
    );
}

#[derive(Debug)]
#[repr(C)]
pub struct SupervisorContext {
    pub ra: usize, // 0
    pub sp: usize,
    pub gp: usize,
    pub tp: usize,
    pub t0: usize,
    pub t1: usize,
    pub t2: usize,
    pub s0: usize,
    pub s1: usize,
    pub a0: usize,
    pub a1: usize,
    pub a2: usize,
    pub a3: usize,
    pub a4: usize,
    pub a5: usize,
    pub a6: usize,
    pub a7: usize,
    pub s2: usize,
    pub s3: usize,
    pub s4: usize,
    pub s5: usize,
    pub s6: usize,
    pub s7: usize,
    pub s8: usize,
    pub s9: usize,
    pub s10: usize,
    pub s11: usize,
    pub t3: usize,
    pub t4: usize,
    pub t5: usize,
    pub t6: usize,        // 30
    pub mstatus: Mstatus, // 31
    pub mepc: usize,      // 32
}
