//! SBI ecall register preservation and illegal-instruction redirection.

use core::arch::{asm, naked_asm};

const ILLEGAL_INSTRUCTION: usize = 2;

pub(crate) fn test() {
    test_ecall_registers();
    test_redirected_traps();
}

macro_rules! check {
    ($what:expr, $got:expr, $want:expr) => {{
        let (got, want) = ($got, $want);
        assert_eq!(got, want, "{}: got {got:#x}, want {want:#x}", $what);
    }};
}

fn test_ecall_registers() {
    // Check the caller-saved t0–t6 and a2–a7 registers across an SBI ecall.
    // a0/a1 carry its result and are intentionally excluded.
    unsafe {
        let magic = |register: usize| 0x5150_0000_0000_0000usize | register;
        let (mut x5, mut x6, mut x7, mut x28, mut x29, mut x30, mut x31): (
            usize,
            usize,
            usize,
            usize,
            usize,
            usize,
            usize,
        ) = (
            magic(5),
            magic(6),
            magic(7),
            magic(28),
            magic(29),
            magic(30),
            magic(31),
        );
        let (mut x12, mut x13, mut x14, mut x15, mut x16, mut x17): (
            usize,
            usize,
            usize,
            usize,
            usize,
            usize,
        ) = (
            magic(12),
            magic(13),
            magic(14),
            magic(15),
            magic(16),
            magic(17),
        );
        // The magic a6/a7 select an unknown extension, so this ecall returns
        // an error while still exercising the complete trap round-trip.
        asm!(
            ".option push",
            ".option norvc",
            "ecall",
            ".option pop",
            inout("t0") x5,
            inout("t1") x6,
            inout("t2") x7,
            inout("t3") x28,
            inout("t4") x29,
            inout("t5") x30,
            inout("t6") x31,
            inout("a2") x12,
            inout("a3") x13,
            inout("a4") x14,
            inout("a5") x15,
            inout("a6") x16,
            inout("a7") x17,
            out("a0") _,
            out("a1") _,
        );
        let preserved = [
            (5, x5),
            (6, x6),
            (7, x7),
            (28, x28),
            (29, x29),
            (30, x30),
            (31, x31),
            (12, x12),
            (13, x13),
            (14, x14),
            (15, x15),
            (16, x16),
            (17, x17),
        ];
        for (register, got) in preserved {
            check!("ecall preserves a register", got, magic(register));
        }
    }
    println!("[trap] ecall register preservation pass");
}

/// Recorded `scause` of the last trap caught by [`s_skip_trap`].
#[unsafe(no_mangle)]
static mut TRAP_CAUSE: usize = 0;

/// S-mode vector used to observe traps redirected by the firmware.
#[unsafe(naked)]
#[unsafe(no_mangle)]
unsafe extern "C" fn s_skip_trap() {
    naked_asm!(
        ".balign 4",
        "csrr t0, scause",
        "bltz t0, 1f",
        "la t2, {cause}",
        "sd t0, 0(t2)",
        "csrr t1, sepc",
        "addi t1, t1, 4",
        "csrw sepc, t1",
        "sret",
        "1:",
        "csrci sip, 2",
        "sret",
        cause = sym TRAP_CAUSE,
    );
}

fn test_redirected_traps() {
    // The firmware refuses to emulate an unknown CSR read. It must redirect
    // that illegal instruction to S-mode while continuing to run.
    unsafe {
        let old_stvec: usize;
        asm!("csrr {value}, stvec", value = out(reg) old_stvec);
        asm!("csrw stvec, {value}", value = in(reg) s_skip_trap as *const () as usize);

        core::ptr::write_volatile(&raw mut TRAP_CAUSE, 0);
        asm!(
            ".option push",
            ".option norvc",
            "csrrs zero, {csr}, zero",
            ".option pop",
            csr = const 0x7c3usize,
            // The temporary S-mode vector uses these registers.
            out("t0") _,
            out("t1") _,
            out("t2") _,
        );
        let cause = core::ptr::read_volatile(&raw const TRAP_CAUSE);
        let mut write_causes = [0; 2];
        for (mask, cause) in [0usize, 1].into_iter().zip(&mut write_causes) {
            core::ptr::write_volatile(&raw mut TRAP_CAUSE, 0);
            // A non-x0 source attempts a write even when its value is zero.
            asm!(
                ".option push",
                ".option norvc",
                "csrrs zero, time, a0",
                ".option pop",
                in("a0") mask,
                out("t0") _,
                out("t1") _,
                out("t2") _,
            );
            *cause = core::ptr::read_volatile(&raw const TRAP_CAUSE);
        }
        // Restore the vector before an assertion can panic.
        asm!("csrw stvec, {value}", value = in(reg) old_stvec);
        assert_eq!(
            cause, ILLEGAL_INSTRUCTION,
            "unknown CSR must redirect to S-mode"
        );
        for cause in write_causes {
            assert_eq!(
                cause, ILLEGAL_INSTRUCTION,
                "a write to time must redirect to S-mode"
            );
        }
    }
    println!("[trap] redirected traps survived");
}
