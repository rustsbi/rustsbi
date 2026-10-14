//! Trap register preservation, counter permissions, and S/HS/VS forwarding.

use core::arch::{asm, naked_asm};

const ILLEGAL_INSTRUCTION: usize = 2;

pub(crate) fn test() {
    test_ecall_registers();
    test_redirected_traps();
    test_user_time_access();
    test_guest_traps();
}

#[cfg(target_pointer_width = "64")]
macro_rules! store {
    ($reg:literal, $slot:literal, $base:literal) => {
        concat!("sd ", $reg, ", 8*", $slot, "(", $base, ")")
    };
}
#[cfg(target_pointer_width = "32")]
macro_rules! store {
    ($reg:literal, $slot:literal, $base:literal) => {
        concat!("sw ", $reg, ", 4*", $slot, "(", $base, ")")
    };
}
#[cfg(target_pointer_width = "64")]
macro_rules! load {
    ($reg:literal, $slot:literal, $base:literal) => {
        concat!("ld ", $reg, ", 8*", $slot, "(", $base, ")")
    };
}
#[cfg(target_pointer_width = "32")]
macro_rules! load {
    ($reg:literal, $slot:literal, $base:literal) => {
        concat!("lw ", $reg, ", 4*", $slot, "(", $base, ")")
    };
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
    // SAFETY: The call uses an unsupported SBI extension and declares all
    // result-register clobbers; other supplied registers must survive the trap.
    unsafe {
        let magic = |register: usize| 0x5150_0000usize | register;
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

#[unsafe(no_mangle)]
static mut TRAP_PC: usize = 0;

#[unsafe(no_mangle)]
static mut TRAP_VALUE: usize = 0;

/// Records a test exception and resumes after its four-byte instruction slot.
///
/// # Safety
///
/// Install only as an S-mode trap vector with writable test records.
/// Test instructions must occupy four-byte slots and account for `t0`/`t1`/`t2` clobbers.
#[unsafe(naked)]
#[unsafe(no_mangle)]
unsafe extern "C" fn s_skip_trap() {
    naked_asm!(
        ".balign 4",
        "csrr t0, scause",
        "bltz t0, 1f",
        "la t2, {cause}",
        store!("t0", "0", "t2"),
        "csrr t1, sepc",
        "la t2, {pc}",
        store!("t1", "0", "t2"),
        "csrr t0, stval",
        "la t2, {value}",
        store!("t0", "0", "t2"),
        "addi t1, t1, 4",
        "csrw sepc, t1",
        "sret",
        "1:",
        "csrci sip, 2",
        "sret",
        cause = sym TRAP_CAUSE,
        pc = sym TRAP_PC,
        value = sym TRAP_VALUE,
    );
}

/// Executes one access to the unmapped gap below CLINT on QEMU virt.
pub(crate) fn access_fault(store: bool) {
    const UNMAPPED: usize = 0x0100_0000;
    // SAFETY: QEMU virt leaves this address unmapped. The temporary vector skips
    // the faulting four-byte instruction, and its register clobbers are declared.
    // Restore stvec before checking the recorded cause, PC, and address.
    let (cause, pc, value, fault_pc) = unsafe {
        let old_vector: usize;
        asm!("csrr {old}, stvec", "csrw stvec, {new}",
            old = out(reg) old_vector, new = in(reg) s_skip_trap as *const () as usize);
        core::ptr::write_volatile(&raw mut TRAP_CAUSE, 0);
        let fault_pc: usize;
        if store {
            asm!(
                ".option push", ".option norvc",
                "lla {pc}, 2f", "2: sw zero, 0({address})", ".option pop",
                address = in(reg) UNMAPPED, pc = out(reg) fault_pc,
                out("t0") _, out("t1") _, out("t2") _,
            );
        } else {
            asm!(
                ".option push", ".option norvc",
                "lla {pc}, 2f", "2: lw zero, 0({address})", ".option pop",
                address = in(reg) UNMAPPED, pc = out(reg) fault_pc,
                out("t0") _, out("t1") _, out("t2") _,
            );
        }
        asm!("csrw stvec, {old}", old = in(reg) old_vector);
        (
            core::ptr::read_volatile(&raw const TRAP_CAUSE),
            core::ptr::read_volatile(&raw const TRAP_PC),
            core::ptr::read_volatile(&raw const TRAP_VALUE),
            fault_pc,
        )
    };
    assert_eq!(cause, if store { 7 } else { 5 });
    assert_eq!(pc, fault_pc);
    assert_eq!(value, UNMAPPED);
}

fn test_redirected_traps() {
    // The firmware refuses to emulate an unknown CSR read. It must redirect
    // that illegal instruction to S-mode while continuing to run.
    // SAFETY: The temporary S-mode vector records only these test traps.
    // Inline assembly declares its scratch registers, and the original vector
    // is restored before any assertion can panic.
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
        // The zero encoding forces the mtval=0 fetch path; redirection must
        // retain the original exception even after those memory accesses.
        core::ptr::write_volatile(&raw mut TRAP_CAUSE, 0);
        core::ptr::write_volatile(&raw mut TRAP_PC, usize::MAX);
        core::ptr::write_volatile(&raw mut TRAP_VALUE, usize::MAX);
        let fault_pc: usize;
        asm!(
            ".option push",
            ".option norvc",
            "lla {pc}, 2f",
            "2: .4byte 0",
            ".option pop",
            pc = out(reg) fault_pc,
            out("t0") _,
            out("t1") _,
            out("t2") _,
        );
        let fetched_cause = core::ptr::read_volatile(&raw const TRAP_CAUSE);
        let fetched_pc = core::ptr::read_volatile(&raw const TRAP_PC);
        let fetched_value = core::ptr::read_volatile(&raw const TRAP_VALUE);
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
        check!(
            "fetched illegal instruction cause",
            fetched_cause,
            ILLEGAL_INSTRUCTION
        );
        check!("fetched illegal instruction PC", fetched_pc, fault_pc);
        check!("fetched illegal instruction value", fetched_value, 0);
    }
    println!("[trap] redirected traps survived");
}

#[repr(C)]
#[derive(Default)]
struct UserTimeTrap {
    cause: usize,
    pc: usize,
    value: usize,
    read_pc: usize,
    saved: [usize; 5],
}

fn test_user_time_access() {
    const TM: usize = 1 << 1;
    // SAFETY: The temporary vector skips a four-byte time read if unavailable.
    // Both stvec and scounteren are restored before leaving this probe.
    let enabled = unsafe {
        let old_vector: usize;
        asm!("csrr {old}, stvec", "csrw stvec, {new}",
            old = out(reg) old_vector, new = in(reg) s_skip_trap as *const () as usize);
        core::ptr::write_volatile(&raw mut TRAP_CAUSE, 0);
        asm!(".option push", ".option norvc", "csrr zero, time", ".option pop",
            out("t0") _, out("t1") _, out("t2") _);
        let readable = core::ptr::read_volatile(&raw const TRAP_CAUSE) == 0;
        asm!("csrw stvec, {old}", old = in(reg) old_vector);
        let enabled: usize;
        asm!("csrr {old}, scounteren", "csrs scounteren, {mask}",
            "csrr {enabled}, scounteren", "csrw scounteren, {old}",
            old = out(reg) _, enabled = out(reg) enabled, mask = in(reg) TM);
        readable && enabled & TM != 0
    };
    for high in 0..if cfg!(target_pointer_width = "32") {
        2
    } else {
        1
    } {
        for allow in [false, true] {
            if allow && !enabled {
                continue;
            }
            let mut facts = UserTimeTrap::default();
            // SAFETY: The assembly runs only rdtime/timeh and ecall in U-mode,
            // then restores saved supervisor control CSRs before returning to Rust.
            unsafe { user_time_roundtrip(&mut facts, if allow { TM } else { 0 }, high) };
            assert_eq!(
                facts.cause,
                if allow { 8 } else { ILLEGAL_INSTRUCTION },
                "U-mode time access: allow={allow}, high={high}"
            );
            if !allow {
                assert_eq!(facts.pc, facts.read_pc, "denied read must retain its PC");
                assert_eq!(
                    facts.value, 0x55aa,
                    "denied read must preserve its destination"
                );
            }
        }
    }
    println!("[trap] U-mode time access restrictions pass");
}

/// Attempts a U-mode time read and returns with its first trap recorded.
///
/// # Safety
///
/// Run in S-mode with bare translation and Runtime trap handling installed.
/// The first argument must exclusively reference a writable `UserTimeTrap`.
/// Select the high time word only on RV32.
#[unsafe(naked)]
unsafe extern "C" fn user_time_roundtrip(_: *mut UserTimeTrap, _: usize, _: usize) {
    naked_asm!(
        "csrr t0, sstatus",
        store!("t0", "4", "a0"),
        "csrr t0, stvec",
        store!("t0", "5", "a0"),
        "csrr t0, sepc",
        store!("t0", "6", "a0"),
        "csrr t0, sie",
        store!("t0", "7", "a0"),
        "csrr t0, scounteren",
        store!("t0", "8", "a0"),
        "csrw sie, zero",
        "csrw scounteren, a1",
        "li t0, (1 << 1) | (1 << 5) | (1 << 8)",
        "csrc sstatus, t0",
        "lla t0, 4f",
        "csrw stvec, t0",
        "lla t0, 2f",
        "beqz a2, 1f",
        "lla t0, 3f",
        "1:",
        store!("t0", "3", "a0"),
        "csrw sepc, t0",
        "li a3, 0x55aa",
        "sret",
        ".option push",
        ".option norvc",
        "2: csrr a3, time",
        "j 6f",
        // The timeh branch is selected only by RV32 tests.
        "3: csrr a3, 0xc81",
        "6: ecall",
        ".balign 4",
        "4:",
        "csrr t0, scause",
        store!("t0", "0", "a0"),
        "csrr t0, sepc",
        store!("t0", "1", "a0"),
        store!("a3", "2", "a0"),
        "li t0, 1 << 8",
        "csrs sstatus, t0",
        "lla t0, 5f",
        "csrw sepc, t0",
        "sret",
        "5:",
        ".option pop",
        load!("t0", "5", "a0"),
        "csrw stvec, t0",
        load!("t0", "6", "a0"),
        "csrw sepc, t0",
        load!("t0", "8", "a0"),
        "csrw scounteren, t0",
        load!("t0", "4", "a0"),
        "csrw sstatus, t0",
        load!("t0", "7", "a0"),
        "csrw sie, t0",
        "ret",
    );
}

#[repr(C)]
#[derive(Debug, Default)]
struct GuestTraps {
    hs_cause: usize,
    hs_pc: usize,
    hs_status: usize,
    hs_value2: usize,
    hs_instruction: usize,
    vs_cause: usize,
    vs_pc: usize,
    illegal_pc: usize,
    ecall_pc: usize,
    vs_status: usize,
    hs_value: usize,
    saved: [usize; 11],
}

fn test_guest_traps() {
    // Probe H through the S-mode skip vector so the same kernel runs without H.
    // SAFETY: The temporary vector skips the four-byte hstatus read if absent;
    // the original vector is restored before the probe returns.
    let available = unsafe {
        let old_vector: usize;
        asm!("csrr {old}, stvec", "csrw stvec, {new}",
            old = out(reg) old_vector, new = in(reg) s_skip_trap as *const () as usize);
        core::ptr::write_volatile(&raw mut TRAP_CAUSE, 0);
        asm!(".option push", ".option norvc", "csrr zero, hstatus", ".option pop",
            out("t0") _, out("t1") _, out("t2") _);
        asm!("csrw stvec, {old}", old = in(reg) old_vector);
        core::ptr::read_volatile(&raw const TRAP_CAUSE) == 0
    };
    if !available {
        println!("[trap] H extension unavailable");
        return;
    }
    for supervisor in [false, true] {
        for delegate in [false, true] {
            let mut facts = GuestTraps::default();
            // SAFETY: The helper uses bare translation, restores saved host control CSRs,
            // and returns through private HS/VS vectors with no Rust in a guest.
            unsafe {
                guest_roundtrip(
                    &mut facts,
                    if delegate { 1 << 2 } else { 0 },
                    usize::from(supervisor) << 8,
                );
            }
            assert_eq!(
                facts.hs_status & (1 << 7),
                1 << 7,
                "HS trap must record V=1"
            );
            assert_eq!(
                facts.hs_status & (1 << 8),
                usize::from(supervisor) << 8,
                "SPVP: supervisor={supervisor}, delegate={delegate}, facts={facts:x?}"
            );
            assert_eq!(
                facts.hs_status & (1 << 6),
                0,
                "instruction faults have no guest address"
            );
            assert_eq!(facts.hs_value2, 0);
            assert_eq!(facts.hs_instruction, 0);
            assert_eq!(facts.hs_value, 0);
            if delegate {
                assert_eq!(facts.vs_cause, ILLEGAL_INSTRUCTION);
                assert_eq!(facts.vs_pc, facts.illegal_pc);
                assert_eq!(facts.vs_status & (1 << 8), usize::from(supervisor) << 8);
                assert_eq!(
                    facts.hs_cause,
                    if supervisor { 10 } else { 8 },
                    "guest ecall belongs to HS"
                );
                assert_eq!(facts.hs_pc, facts.ecall_pc);
            } else {
                assert_eq!(facts.vs_cause, 0);
                assert_eq!(facts.hs_cause, ILLEGAL_INSTRUCTION);
                assert_eq!(facts.hs_pc, facts.illegal_pc);
            }
        }
    }
    println!("[trap] HS/VS routing pass");
}

/// Runs a guest exception and ecall through private HS/VS test vectors.
///
/// # Safety
///
/// Run in HS-mode on a hart with H and bare host translation.
/// `a0` must exclusively reference `GuestTraps` writable from the bare HS/VS contexts.
/// `a1` supplies the `hedeleg` mask; `a2` supplies only the initial `SPP` bit.
#[unsafe(naked)]
unsafe extern "C" fn guest_roundtrip(_: *mut GuestTraps, _: usize, _: usize) {
    naked_asm!(
        "csrr t0, hstatus",
        store!("t0", "11", "a0"),
        "csrr t0, hedeleg",
        store!("t0", "12", "a0"),
        "csrr t0, vstvec",
        store!("t0", "13", "a0"),
        "csrr t0, vsstatus",
        store!("t0", "14", "a0"),
        "csrr t0, vsatp",
        store!("t0", "15", "a0"),
        "csrr t0, hgatp",
        store!("t0", "16", "a0"),
        "csrr t0, stvec",
        store!("t0", "17", "a0"),
        "csrr t0, sepc",
        store!("t0", "18", "a0"),
        "csrr t0, sstatus",
        store!("t0", "19", "a0"),
        "csrr t0, sie",
        store!("t0", "20", "a0"),
        "csrr t0, vsie",
        store!("t0", "21", "a0"),
        "csrw sie, zero",
        "csrw vsie, zero",
        "li t0, (1 << 1) | (1 << 5) | (1 << 8)",
        "csrc sstatus, t0",
        "csrs sstatus, a2",
        "li t0, (1 << 7) | (1 << 8)",
        "csrs hstatus, t0",
        "csrw hedeleg, a1",
        "csrw vsstatus, zero",
        "csrw vsatp, zero",
        "csrw hgatp, zero",
        "hfence.gvma",
        "hfence.vvma",
        "lla t0, 3f",
        "csrw stvec, t0",
        "lla t0, 4f",
        "csrw vstvec, t0",
        "lla t0, 2f",
        "csrw sepc, t0",
        store!("t0", "7", "a0"),
        "lla t0, 5f",
        store!("t0", "8", "a0"),
        "sret",
        ".option push",
        ".option norvc",
        "2: .4byte 0",
        "5: ecall",
        ".balign 4",
        // HS captures the forwarded metadata, then leaves the guest.
        "3:",
        "csrr t0, scause",
        store!("t0", "0", "a0"),
        "csrr t0, sepc",
        store!("t0", "1", "a0"),
        "csrr t0, hstatus",
        store!("t0", "2", "a0"),
        "csrr t0, htval",
        store!("t0", "3", "a0"),
        "csrr t0, htinst",
        store!("t0", "4", "a0"),
        "csrr t0, stval",
        store!("t0", "10", "a0"),
        "li t0, 1 << 7",
        "csrc hstatus, t0",
        "li t0, 1 << 8",
        "csrs sstatus, t0",
        "lla t0, 6f",
        "csrw sepc, t0",
        "sret",
        ".balign 4",
        // VS handles the delegated illegal instruction and resumes its guest.
        "4:",
        "csrr t0, scause",
        store!("t0", "5", "a0"),
        "csrr t0, sepc",
        store!("t0", "6", "a0"),
        "csrr t1, sstatus",
        store!("t1", "9", "a0"),
        "addi t0, t0, 4",
        "csrw sepc, t0",
        "sret",
        "6:",
        ".option pop",
        load!("t0", "11", "a0"),
        "csrw hstatus, t0",
        load!("t0", "12", "a0"),
        "csrw hedeleg, t0",
        load!("t0", "13", "a0"),
        "csrw vstvec, t0",
        load!("t0", "14", "a0"),
        "csrw vsstatus, t0",
        load!("t0", "15", "a0"),
        "csrw vsatp, t0",
        load!("t0", "16", "a0"),
        "csrw hgatp, t0",
        "hfence.gvma",
        "hfence.vvma",
        load!("t0", "17", "a0"),
        "csrw stvec, t0",
        load!("t0", "18", "a0"),
        "csrw sepc, t0",
        load!("t0", "20", "a0"),
        "csrw sie, t0",
        load!("t0", "21", "a0"),
        "csrw vsie, t0",
        load!("t0", "19", "a0"),
        "csrw sstatus, t0",
        "ret",
    );
}
