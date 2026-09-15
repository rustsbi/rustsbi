//! The machine trap entries: the normal and recovery assembly entries.
//!
//! Two naked trap vectors implementing the normal and recovery entry paths:
//!
//! - [`trap_entry`]: the normal vector. `mscratch` is either the zero
//!   sentinel (M-origin while not Armed: reverse the swap and fail-stop) or
//!   the Runtime stack top (lower-origin trap: allocate the frame below the
//!   top, save everything, zero `mscratch`, and call the Rust dispatch).
//! - [`recovery_entry`]: the expected-fault vector for guarded operations.
//!   It validates the active `RecoveryRecord` (exact `mepc`, allowed
//!   `mcause`, M-origin), records the facts, and skips the faulting
//!   instruction; every mismatch fail-stops.
//!
//! Both park into [`crate::boot::fail_stop`], the boot domain's
//! stack-independent vector.

use super::dispatch::trap_dispatch;
use super::frame::offsets;
use super::recovery::RecoveryRecord;

const FRAME_BYTES: usize =
    (offsets::SIZE_WORDS * core::mem::size_of::<usize>()).next_multiple_of(16);
const _: () = assert!(FRAME_BYTES.is_multiple_of(16));
#[cfg(target_pointer_width = "32")]
const _: () = assert!(FRAME_BYTES == 144);
#[cfg(target_pointer_width = "64")]
const _: () = assert!(FRAME_BYTES == 288);

#[cfg(target_pointer_width = "32")]
macro_rules! save_word {
    ($reg:ident => $base:ident[$offset:expr]) => {
        concat!(
            "sw ",
            stringify!($reg),
            ", 4*",
            $offset,
            "(",
            stringify!($base),
            ")"
        )
    };
}

#[cfg(target_pointer_width = "32")]
macro_rules! load_word {
    ($base:ident[$offset:expr] => $reg:ident) => {
        concat!(
            "lw ",
            stringify!($reg),
            ", 4*",
            $offset,
            "(",
            stringify!($base),
            ")"
        )
    };
}

#[cfg(target_pointer_width = "64")]
macro_rules! save_word {
    ($reg:ident => $base:ident[$offset:expr]) => {
        concat!(
            "sd ",
            stringify!($reg),
            ", 8*",
            $offset,
            "(",
            stringify!($base),
            ")"
        )
    };
}

#[cfg(target_pointer_width = "64")]
macro_rules! load_word {
    ($base:ident[$offset:expr] => $reg:ident) => {
        concat!(
            "ld ",
            stringify!($reg),
            ", 8*",
            $offset,
            "(",
            stringify!($base),
            ")"
        )
    };
}

/// The normal machine trap vector (direct mode).
///
/// # Safety
///
/// Naked `mtvec` target, never a callable function. Requires `mscratch` to
/// hold either the zero sentinel or the Runtime stack top, as maintained by
/// this module's protocol.
#[unsafe(naked)]
#[unsafe(export_name = "runtime_trap_entry")]
pub unsafe extern "C" fn trap_entry() {
    core::arch::naked_asm!(
        ".align 2",
        // Symmetric swap: sp ← mscratch (Runtime stack top), mscratch ←
        // trapped sp. No register has been touched yet.
        "csrrw sp, mscratch, sp",
        "bnez sp, 1f",
        // Zero sentinel: an M-origin trap outside the Armed windows. Reverse
        // the swap without touching memory and fail-stop.
        "csrrw sp, mscratch, sp",
        "j {fail_stop}",
        // Lower-origin trap: sp = the clean stack top; allocate the frame.
        "1:",
        "addi sp, sp, -{frame_size}",
        // Save caller-saved registers first; defer s0-s11 until needed.
        // Slots omit x0: register xN uses sp[N - 1], so ra (x1) uses sp[0].
        save_word!(ra => sp[0]),
        save_word!(gp => sp[2]),
        save_word!(tp => sp[3]),
        save_word!(t0 => sp[4]),
        save_word!(t1 => sp[5]),
        save_word!(t2 => sp[6]),
        save_word!(a0 => sp[9]),
        save_word!(a1 => sp[10]),
        save_word!(a2 => sp[11]),
        save_word!(a3 => sp[12]),
        save_word!(a4 => sp[13]),
        save_word!(a5 => sp[14]),
        save_word!(a6 => sp[15]),
        save_word!(a7 => sp[16]),
        save_word!(t3 => sp[27]),
        save_word!(t4 => sp[28]),
        save_word!(t5 => sp[29]),
        save_word!(t6 => sp[30]),
        // The trapped x2 rides in mscratch; store it into its frame slot.
        "csrr t0, mscratch",
        save_word!(t0 => sp[1]),
        // Save the trap CSRs for Rust inspection; the live CSRs keep the
        // trap facts until Rust commits an effect.
        "csrr t0, mepc",
        save_word!(t0 => sp[31]),
        "csrr t0, mstatus",
        save_word!(t0 => sp[32]),
        "csrr t0, mcause",
        save_word!(t0 => sp[33]),
        "csrr t0, mtval",
        save_word!(t0 => sp[34]),
        // Zero the sentinel before entering Runtime Rust; a nested trap now
        // fail-stops through the sentinel branch.
        "csrw mscratch, zero",
        "csrr t0, mcause",
        "li t1, 2",
        "bne t0, t1, 2f",
        "mv a0, sp",
        "call {fast_time}",
        "bnez a0, 3f",
        "2:",
        save_word!(s0 => sp[7]),
        save_word!(s1 => sp[8]),
        save_word!(s2 => sp[17]),
        save_word!(s3 => sp[18]),
        save_word!(s4 => sp[19]),
        save_word!(s5 => sp[20]),
        save_word!(s6 => sp[21]),
        save_word!(s7 => sp[22]),
        save_word!(s8 => sp[23]),
        save_word!(s9 => sp[24]),
        save_word!(s10 => sp[25]),
        save_word!(s11 => sp[26]),
        "mv a0, sp",
        "call {dispatch}",
        load_word!(sp[7] => s0),
        load_word!(sp[8] => s1),
        load_word!(sp[17] => s2),
        load_word!(sp[18] => s3),
        load_word!(sp[19] => s4),
        load_word!(sp[20] => s5),
        load_word!(sp[21] => s6),
        load_word!(sp[22] => s7),
        load_word!(sp[23] => s8),
        load_word!(sp[24] => s9),
        load_word!(sp[25] => s10),
        load_word!(sp[26] => s11),
        "3:",
        // Return path: publish the clean top into mscratch first, so no
        // untrusted lower pointer is ever interpreted as a Runtime stack.
        "addi t1, sp, {frame_size}",
        "csrw mscratch, t1",
        // Restore the remaining saved registers except x2 (sp); t0/t1 restore normally —
        // the trapped x2 loads last, directly into sp: the base uses the
        // frame pointer for this final access, so no scratch register is
        // consumed and none of the restored values is clobbered.
        // Slots omit x0: register xN uses sp[N - 1], so ra (x1) uses sp[0].
        load_word!(sp[0] => ra),
        load_word!(sp[2] => gp),
        load_word!(sp[3] => tp),
        load_word!(sp[6] => t2),
        load_word!(sp[9] => a0),
        load_word!(sp[10] => a1),
        load_word!(sp[11] => a2),
        load_word!(sp[12] => a3),
        load_word!(sp[13] => a4),
        load_word!(sp[14] => a5),
        load_word!(sp[15] => a6),
        load_word!(sp[16] => a7),
        load_word!(sp[27] => t3),
        load_word!(sp[28] => t4),
        load_word!(sp[29] => t5),
        load_word!(sp[30] => t6),
        // Restore t0, then use it to carry the trapped x2 across the stack
        // switch; t1 keeps t0's trapped value alive through the move.
        load_word!(sp[4] => t0),
        load_word!(sp[5] => t1),
        load_word!(sp[1] => sp),
        "mret",
        frame_size = const FRAME_BYTES,
        dispatch = sym trap_dispatch,
        fast_time = sym super::dispatch::try_fast_emulate_time,
        fail_stop = sym crate::boot::fail_stop,
    );
}

/// The expected-fault recovery vector for guarded machine operations.
///
/// A recovery is valid only when the active record is published in
/// `mscratch`, the nested trap came from M-mode, `mepc` is exactly the
/// registered faulting instruction, and `mcause` is in the record's allowed
/// set. Accepted faults record their facts and skip the instruction; every
/// mismatch fail-stops.
///
/// # Safety
///
/// Naked `mtvec` target, never a callable function.
#[unsafe(naked)]
#[unsafe(export_name = "runtime_recovery_entry")]
pub(crate) unsafe extern "C" fn recovery_entry() {
    core::arch::naked_asm!(
        ".align 2",
        // The record must be published; mscratch is zero otherwise.
        "csrr t0, mscratch",
        "beqz t0, {fail_stop}",
        // The guarded instruction executes in M-mode, so the nested trap's
        // previous privilege must be M.
        "csrr t1, mstatus",
        "srli t1, t1, 11",
        "andi t1, t1, 3",
        "li t2, 3",
        "bne t1, t2, {fail_stop}",
        // Exact faulting-instruction match.
        "csrr t1, mepc",
        load_word!(t0[0] => t2),
        "bne t1, t2, {fail_stop}",
        // Allowed-cause match: exceptions only (interrupt bit clear), the
        // cause's bit must be set in the record's mask.
        "csrr t1, mcause",
        "bltz t1, {fail_stop}",
        "li t2, 32",
        "bgeu t1, t2, {fail_stop}",
        "li t2, 1",
        "sll t3, t2, t1",
        load_word!(t0[1] => t2),
        "and t3, t3, t2",
        "beqz t3, {fail_stop}",
        // Accepted: record the facts and skip the 4-byte guarded
        // instruction (guards force `.option norvc`).
        "csrr t1, mepc",
        save_word!(t1 => t0[3]),
        "csrr t1, mcause",
        save_word!(t1 => t0[4]),
        "csrr t1, mtval",
        save_word!(t1 => t0[5]),
        "li t1, 1",
        save_word!(t1 => t0[2]),
        "csrr t1, mepc",
        "addi t1, t1, 4",
        "csrw mepc, t1",
        "mret",
        fail_stop = sym crate::boot::fail_stop,
    );
}

const _: () = assert!(RecoveryRecord::WORDS == 6);
