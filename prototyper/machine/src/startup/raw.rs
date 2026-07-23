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
        include_str!("raw_dynamic.S"),
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

/// Raw entry for a next stage selected by the image build configuration.
///
/// # Safety
///
/// The architecture entry ABI supplies `a0 = hart_id` and `a1 = dtb_address`.
/// The previous stage must satisfy the stable readable DTB-envelope contract.
#[unsafe(naked)]
#[cfg(any(feature = "jump", feature = "payload"))]
pub unsafe extern "C" fn raw_entry() -> ! {
    core::arch::naked_asm!(
        include_str!("raw_configured.S"),
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
        cold_configured_entry = sym super::runtime::cold_configured_entry,
    )
}
