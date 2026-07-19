//! Stackless physical-hart lookup used by raw RISC-V entry.

use super::{HART_MAP, PublishedHartMap};
use crate::config::HART_CAPACITY;

#[cfg(target_pointer_width = "64")]
macro_rules! load_register {
    () => {
        "ld"
    };
}

#[cfg(target_pointer_width = "32")]
macro_rules! load_register {
    () => {
        "lw"
    };
}

/// Resolves the arriving physical hart ID without touching a stack.
///
/// # Safety
///
/// The caller must have acquired final runtime publication, proving the map is
/// complete and immutable.
#[unsafe(naked)]
pub(crate) unsafe extern "C" fn entry_index(_hart_id: usize) -> usize {
    core::arch::naked_asm!(
        "lla t0, {map}",
        concat!(load_register!(), " t1, {len_offset}(t0)"),
        "addi t0, t0, {ids_offset}",
        "li t2, 0",
        "1:",
        "bgeu t2, t1, 2f",
        concat!(load_register!(), " t3, 0(t0)"),
        "beq t3, a0, 3f",
        "addi t0, t0, {word_size}",
        "addi t2, t2, 1",
        "j 1b",
        "2:",
        "li a0, -1",
        "ret",
        "3:",
        "mv a0, t2",
        "ret",
        map = sym HART_MAP,
        len_offset = const core::mem::offset_of!(PublishedHartMap<HART_CAPACITY>, len),
        ids_offset = const core::mem::offset_of!(PublishedHartMap<HART_CAPACITY>, ids),
        word_size = const core::mem::size_of::<usize>(),
    )
}
