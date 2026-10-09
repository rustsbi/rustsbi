//! SBI ecall register preservation and illegal-instruction redirection.

use core::arch::{asm, naked_asm};

const ILLEGAL_INSTRUCTION: usize = 2;

pub(crate) fn test() {
    test_ecall_registers();
    test_redirected_traps();
    test_user_time_permissions();
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
    // SAFETY:
    // 1. The unsupported SBI call takes no memory pointers and returns on the
    //    same stack.
    // 2. All probe and result registers are declared outputs.
    unsafe {
        let magic = |register: usize| (0x5150usize << (usize::BITS - 16)) | register;
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

/// Records and skips a synchronous trap or acknowledges a software interrupt.
///
/// # Safety
///
/// Only the boot hart may install this vector and access `TRAP_CAUSE`.
/// Synchronous traps must come from four-byte instructions that can safely
/// be skipped. Interrupted code must permit clobbering `t0`–`t2`.
/// Asynchronous traps must be disabled or restricted to supervisor software
/// interrupts while this vector is installed.
#[unsafe(naked)]
#[unsafe(no_mangle)]
unsafe extern "C" fn s_skip_trap() {
    naked_asm!(
        ".balign 4",
        "csrr t0, scause",
        "bltz t0, 1f",
        "la t2, {cause}",
        ".if {XLEN} == 64",
        "sd t0, 0(t2)",
        ".else",
        "sw t0, 0(t2)",
        ".endif",
        "csrr t1, sepc",
        "addi t1, t1, 4",
        "csrw sepc, t1",
        "sret",
        "1:",
        "csrci sip, 2",
        "sret",
        cause = sym TRAP_CAUSE,
        XLEN = const usize::BITS,
    );
}

fn test_redirected_traps() {
    // The firmware refuses to emulate an unknown CSR read. It must redirect
    // that illegal instruction to S-mode while continuing to run.
    // SAFETY:
    // 1. Only the boot hart accesses `TRAP_CAUSE` and the temporary vector.
    // 2. The probes are four-byte CSR instructions and declare the vector's
    //    `t0`–`t2` clobbers. Skipping them requires no result value.
    // 3. Boot and the preceding trap-based SBI tests leave SIE clear. The
    //    original vector is restored before assertions can panic.
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

/// Returns a one-instruction U-mode probe to its S-mode caller.
///
/// # Safety
///
/// Only the boot hart may install this vector and access `TRAP_CAUSE`.
/// Supervisor interrupts must be masked in `sie`, and `sscratch` must hold
/// the S-mode continuation. The interrupted code must permit clobbering
/// `t0`–`t2` and run in Bare mode on its existing writable stack.
#[unsafe(naked)]
unsafe extern "C" fn s_user_return_trap() {
    naked_asm!(
        ".balign 4",
        "csrr t0, scause",
        "la t2, {cause}",
        ".if {XLEN} == 64",
        "sd t0, 0(t2)",
        ".else",
        "sw t0, 0(t2)",
        ".endif",
        "csrr t1, sscratch",
        "csrw sepc, t1",
        "li t0, 256",
        "csrs sstatus, t0",
        "sret",
        cause = sym TRAP_CAUSE,
        XLEN = const usize::BITS,
    );
}

fn test_user_time_permissions() {
    const ECALL_FROM_USER: usize = 8;
    const TIME_ENABLE: usize = 1 << 1;
    macro_rules! probe {
        ($instruction:literal, $destination:tt) => {{
            core::ptr::write_volatile(&raw mut TRAP_CAUSE, 0);
            asm!(
                ".option push",
                ".option norvc",
                "lla t0, 2f",
                "csrw sepc, t0",
                "lla t0, 3f",
                "csrw sscratch, t0",
                "li t0, 288", // SPP | SPIE; return to U with interrupts masked.
                "csrc sstatus, t0",
                "sret",
                "2:",
                $instruction,
                "ecall", // Reached only when the time read was permitted.
                "3:",
                ".option pop",
                out($destination) _,
                out("t0") _,
                out("t1") _,
                out("t2") _,
            );
            core::ptr::read_volatile(&raw const TRAP_CAUSE)
        }};
    }
    // SAFETY:
    // 1. Only the boot hart accesses `TRAP_CAUSE` and installs this vector.
    // 2. Each probe keeps translation Bare, masks supervisor interrupts in
    //    `sie`, and returns to its declared continuation on the same stack.
    // 3. All register clobbers are declared.
    // 4. All saved supervisor CSRs, including the original interrupt state,
    //    are restored before assertions.
    let (denied, enabled, allowed) = unsafe {
        let (old_sie, old_stvec, old_scounteren, old_sstatus, old_sepc, old_sscratch): (
            usize,
            usize,
            usize,
            usize,
            usize,
            usize,
        );
        asm!(
            "csrrw {sie}, sie, zero",
            "csrr {stvec}, stvec",
            "csrr {scounteren}, scounteren",
            "csrr {sstatus}, sstatus",
            "csrr {sepc}, sepc",
            "csrr {sscratch}, sscratch",
            sie = out(reg) old_sie,
            stvec = out(reg) old_stvec,
            scounteren = out(reg) old_scounteren,
            sstatus = out(reg) old_sstatus,
            sepc = out(reg) old_sepc,
            sscratch = out(reg) old_sscratch,
        );
        asm!("csrw stvec, {vector}", vector = in(reg) s_user_return_trap as *const () as usize);
        asm!("csrw scounteren, {value}", value = in(reg) old_scounteren & !TIME_ENABLE);
        let denied = [
            probe!("csrrs a0, time, zero", "a0"),
            probe!("csrrs s2, time, zero", "s2"),
        ];
        asm!("csrw scounteren, {value}", value = in(reg) old_scounteren | TIME_ENABLE);
        let enabled = riscv::register::scounteren::read().bits() & TIME_ENABLE != 0;
        let allowed = [
            probe!("csrrs a0, time, zero", "a0"),
            probe!("csrrs s2, time, zero", "s2"),
        ];
        asm!(
            "csrw sscratch, {sscratch}",
            "csrw sepc, {sepc}",
            "csrw scounteren, {scounteren}",
            "csrw stvec, {stvec}",
            "csrw sstatus, {sstatus}",
            "csrw sie, {sie}",
            sscratch = in(reg) old_sscratch,
            sepc = in(reg) old_sepc,
            scounteren = in(reg) old_scounteren,
            stvec = in(reg) old_stvec,
            sstatus = in(reg) old_sstatus,
            sie = in(reg) old_sie,
        );
        (denied, enabled, allowed)
    };
    assert_eq!(
        denied, [ILLEGAL_INSTRUCTION; 2],
        "U-mode time reads must respect scounteren.TM"
    );
    assert_eq!(
        allowed,
        [if enabled {
            ECALL_FROM_USER
        } else {
            ILLEGAL_INSTRUCTION
        }; 2],
        "enabled U-mode time reads must reach their ecall continuation"
    );
    println!("[trap] U-mode time permissions pass (fast and full-frame destinations)");
}
