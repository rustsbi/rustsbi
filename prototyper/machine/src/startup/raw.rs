//! Raw CPU arrival and the transition onto initialized machine stacks.

#[cfg(not(any(feature = "jump", feature = "payload")))]
use crate::boot;
use crate::config::BOOT_STACK_SIZE;

use super::stacks::{BOOT_STACK, HART_STACKS, HartStack};
#[cfg(not(any(feature = "jump", feature = "payload")))]
use super::state::{DYNAMIC_SNAPSHOT, DYNAMIC_WORD_COUNT};
use super::state::{
    EARLY_FAILED, EARLY_INITIALIZING, EARLY_READY, EARLY_STATE, RUNTIME_READY, RUNTIME_STATE,
};

const MSTATUS_MPRV: usize = 1 << 17;

#[cfg(all(
    target_pointer_width = "64",
    not(any(feature = "jump", feature = "payload"))
))]
macro_rules! load_register {
    () => {
        "ld"
    };
}

#[cfg(all(
    target_pointer_width = "32",
    not(any(feature = "jump", feature = "payload"))
))]
macro_rules! load_register {
    () => {
        "lw"
    };
}

#[cfg(target_pointer_width = "64")]
macro_rules! store_register {
    () => {
        "sd"
    };
}

#[cfg(target_pointer_width = "32")]
macro_rules! store_register {
    () => {
        "sw"
    };
}

#[cfg(not(any(feature = "jump", feature = "payload")))]
macro_rules! load_dynamic_words {
    () => {
        concat!(
            load_register!(),
            " s3, 0 * {word_size}(s2)\n",
            load_register!(),
            " s4, 1 * {word_size}(s2)\n",
            load_register!(),
            " s5, 2 * {word_size}(s2)\n",
            load_register!(),
            " s6, 3 * {word_size}(s2)\n",
            load_register!(),
            " s7, 4 * {word_size}(s2)\n",
            load_register!(),
            " s8, 5 * {word_size}(s2)"
        )
    };
}

#[cfg(not(any(feature = "jump", feature = "payload")))]
macro_rules! compare_dynamic_snapshot {
    () => {
        concat!(
            "lla t0, {dynamic_snapshot}\n",
            load_register!(),
            " t1, 0 * {word_size}(t0)\n",
            "bne t1, s3, 90f\n",
            load_register!(),
            " t1, 1 * {word_size}(t0)\n",
            "bne t1, s4, 90f\n",
            load_register!(),
            " t1, 2 * {word_size}(t0)\n",
            "bne t1, s5, 90f\n",
            load_register!(),
            " t1, 3 * {word_size}(t0)\n",
            "bne t1, s6, 90f\n",
            load_register!(),
            " t1, 4 * {word_size}(t0)\n",
            "bne t1, s7, 90f\n",
            load_register!(),
            " t1, 5 * {word_size}(t0)\n",
            "bne t1, s8, 90f"
        )
    };
}

macro_rules! clear_bss {
    () => {
        concat!(
            "lla t0, sbi_bss_start\n",
            "lla t1, sbi_bss_end\n",
            "10:\n",
            "bgeu t0, t1, 11f\n",
            store_register!(),
            " zero, 0(t0)\n",
            "addi t0, t0, {word_size}\n",
            "j 10b\n",
            "11:"
        )
    };
}

macro_rules! enter_warm_runtime {
    () => {
        concat!(
            "40:\n",
            "lla t0, {runtime_state}\n",
            "amoadd.w.aq t1, zero, (t0)\n",
            "beqz t1, 40b\n",
            "li t2, {runtime_ready}\n",
            "bne t1, t2, 90f\n",
            "mv a0, s0\n",
            "call {hart_entry_index}\n",
            "li t0, -1\n",
            "beq a0, t0, 90f\n",
            "mv s1, a0\n",
            "lla t0, {hart_stacks}\n",
            "li t1, {hart_stack_stride}\n",
            "li t2, 0\n",
            "mv t3, s1\n",
            "41:\n",
            "beqz t3, 42f\n",
            "add t2, t2, t1\n",
            "addi t3, t3, -1\n",
            "j 41b\n",
            "42:\n",
            "add sp, t0, t2\n",
            "add sp, sp, t1\n",
            "andi sp, sp, -16\n",
            "mv a0, s0\n",
            "mv a1, s1\n",
            "tail {warm_entry}"
        )
    };
}

/// Fixed raw entry target referenced only by the generated entry shim.
///
/// # Safety
///
/// The architecture entry ABI supplies `a0 = hart_id`, `a1 = dtb_address`, and
/// `a2 = dynamic_info_address`. The previous stage must satisfy the stable
/// readable envelope contracts for `a1` and `a2`.
#[unsafe(naked)]
#[cfg(not(any(feature = "jump", feature = "payload")))]
pub unsafe extern "C" fn raw_entry() -> ! {
    core::arch::naked_asm!(
        ".option push",
        ".option arch, +a",
        "csrw mie, zero",
        "li t0, {mprv}",
        "csrc mstatus, t0",
        "mv s0, a0",
        "mv s1, a1",
        "mv s2, a2",
        "beqz s2, 90f",
        "andi t0, s2, {xlen_align_mask}",
        "bnez t0, 90f",
        "addi t0, s2, {dynamic_size}",
        "bltu t0, s2, 90f",
        load_dynamic_words!(),
        "li t0, {dynamic_magic}",
        "bne s3, t0, 90f",
        "li t0, 1",
        "beq s4, t0, 2f",
        "li t0, 2",
        "bne s4, t0, 90f",
        "li t0, -1",
        "beq s8, t0, 2f",
        "bne s0, s8, 3f",
        "2:",
        "lla t0, {early_state}",
        "20:",
        "lr.w.aq t2, (t0)",
        "bnez t2, 3f",
        "li t1, {initializing}",
        "sc.w.rl t2, t1, (t0)",
        "bnez t2, 20b",
        "call {relocation_update}",
        clear_bss!(),
        "lla sp, {boot_stack}",
        "li t0, {boot_stack_size}",
        "add sp, sp, t0",
        "andi sp, sp, -16",
        "mv a0, s0",
        "mv a1, s1",
        "mv a2, s3",
        "mv a3, s4",
        "mv a4, s5",
        "mv a5, s6",
        "mv a6, s7",
        "mv a7, s8",
        "tail {cold_entry}",
        "3:",
        "lla t0, {early_state}",
        "30:",
        "lla t3, {early_failed}",
        "amoadd.w.aq t1, zero, (t3)",
        "bnez t1, 90f",
        "amoadd.w.aq t1, zero, (t0)",
        "li t2, {ready}",
        "bne t1, t2, 30b",
        compare_dynamic_snapshot!(),
        enter_warm_runtime!(),
        "90:",
        "lla t0, {early_failed}",
        "li t1, 1",
        "amoswap.w.rl zero, t1, (t0)",
        "91:",
        "wfi",
        "j 91b",
        ".option pop",
        mprv = const MSTATUS_MPRV,
        xlen_align_mask = const core::mem::size_of::<usize>() - 1,
        dynamic_size = const DYNAMIC_WORD_COUNT * core::mem::size_of::<usize>(),
        word_size = const core::mem::size_of::<usize>(),
        dynamic_magic = const boot::DYNAMIC_MAGIC,
        initializing = const EARLY_INITIALIZING,
        ready = const EARLY_READY,
        early_state = sym EARLY_STATE,
        early_failed = sym EARLY_FAILED,
        runtime_state = sym RUNTIME_STATE,
        runtime_ready = const RUNTIME_READY,
        hart_entry_index = sym crate::hart::entry_index,
        hart_stacks = sym HART_STACKS,
        hart_stack_stride = const core::mem::size_of::<HartStack>(),
        warm_entry = sym super::runtime::warm_entry,
        dynamic_snapshot = sym DYNAMIC_SNAPSHOT,
        relocation_update = sym super::relocation::relocation_update,
        boot_stack = sym BOOT_STACK,
        boot_stack_size = const BOOT_STACK_SIZE,
        cold_entry = sym super::runtime::cold_entry,
    )
}

/// Fixed-address raw entry used by jump and embedded-payload images.
///
/// # Safety
///
/// The architecture entry ABI supplies `a0 = hart_id` and `a1 = dtb_address`.
/// The previous stage must satisfy the stable readable DTB-envelope contract.
#[unsafe(naked)]
#[cfg(any(feature = "jump", feature = "payload"))]
pub unsafe extern "C" fn raw_entry() -> ! {
    core::arch::naked_asm!(
        ".option push",
        ".option arch, +a",
        "csrw mie, zero",
        "li t0, {mprv}",
        "csrc mstatus, t0",
        "mv s0, a0",
        "mv s1, a1",
        "lla t0, {early_state}",
        "20:",
        "lr.w.aq t2, (t0)",
        "bnez t2, 3f",
        "li t1, {initializing}",
        "sc.w.rl t2, t1, (t0)",
        "bnez t2, 20b",
        "call {relocation_update}",
        clear_bss!(),
        "lla sp, {boot_stack}",
        "li t0, {boot_stack_size}",
        "add sp, sp, t0",
        "andi sp, sp, -16",
        "mv a0, s0",
        "mv a1, s1",
        "tail {cold_fixed_entry}",
        "3:",
        "lla t0, {early_state}",
        "30:",
        "lla t3, {early_failed}",
        "amoadd.w.aq t1, zero, (t3)",
        "bnez t1, 90f",
        "amoadd.w.aq t1, zero, (t0)",
        "li t2, {ready}",
        "bne t1, t2, 30b",
        enter_warm_runtime!(),
        "90:",
        "lla t0, {early_failed}",
        "li t1, 1",
        "amoswap.w.rl zero, t1, (t0)",
        "91:",
        "wfi",
        "j 91b",
        ".option pop",
        mprv = const MSTATUS_MPRV,
        word_size = const core::mem::size_of::<usize>(),
        initializing = const EARLY_INITIALIZING,
        ready = const EARLY_READY,
        early_state = sym EARLY_STATE,
        early_failed = sym EARLY_FAILED,
        runtime_state = sym RUNTIME_STATE,
        runtime_ready = const RUNTIME_READY,
        hart_entry_index = sym crate::hart::entry_index,
        hart_stacks = sym HART_STACKS,
        hart_stack_stride = const core::mem::size_of::<HartStack>(),
        warm_entry = sym super::runtime::warm_entry,
        relocation_update = sym super::relocation::relocation_update,
        boot_stack = sym BOOT_STACK,
        boot_stack_size = const BOOT_STACK_SIZE,
        cold_fixed_entry = sym super::runtime::cold_fixed_entry,
    )
}
