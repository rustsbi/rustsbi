//! Previous-firmware dynamic handoff.

use core::sync::atomic::{AtomicUsize, Ordering};

use crate::boot::import::{DYNAMIC_MAGIC, DynamicWords};
use crate::config::BOOT_STACK_SIZE;

use super::super::stacks::{BOOT_STACK, HART_STACKS, HartStack};
use super::super::{allocator, contract, relocation, runtime, state};

const MSTATUS_MPRV: usize = 1 << 17;
const DYNAMIC_WORD_COUNT: usize = 6;

static DYNAMIC_SNAPSHOT: [AtomicUsize; DYNAMIC_WORD_COUNT] =
    [const { AtomicUsize::new(0) }; DYNAMIC_WORD_COUNT];

/// Raw entry for a previous-firmware dynamic handoff.
///
/// # Safety
///
/// The entry ABI supplies `a0 = hart_id`, `a1 = dtb_address`, and
/// `a2 = dynamic_info_address`, each satisfying its documented envelope.
#[unsafe(naked)]
pub(super) unsafe extern "C" fn entry() -> ! {
    core::arch::naked_asm!(
        include_str!("fw_dynamic.S"),
        mprv = const MSTATUS_MPRV,
        xlen_align_mask = const core::mem::size_of::<usize>() - 1,
        dynamic_size = const DYNAMIC_WORD_COUNT * core::mem::size_of::<usize>(),
        word_size = const core::mem::size_of::<usize>(),
        dynamic_magic = const DYNAMIC_MAGIC,
        initializing = const state::EARLY_INITIALIZING,
        ready = const state::EARLY_READY,
        early_state = sym state::EARLY_STATE,
        early_failed = sym state::EARLY_FAILED,
        runtime_state = sym state::RUNTIME_STATE,
        runtime_ready = const state::RUNTIME_READY,
        hart_entry_index = sym crate::hart::entry_index,
        hart_stacks = sym HART_STACKS,
        hart_stack_stride = const core::mem::size_of::<HartStack>(),
        warm_entry = sym runtime::warm_entry,
        dynamic_snapshot = sym DYNAMIC_SNAPSHOT,
        relocation_update = sym relocation::relocation_update,
        boot_stack = sym BOOT_STACK,
        boot_stack_size = const BOOT_STACK_SIZE,
        cold_entry = sym cold_entry,
    )
}

extern "C" fn cold_entry(
    hart_id: usize,
    dtb_address: usize,
    magic: usize,
    version: usize,
    next_address: usize,
    next_mode: usize,
    options: usize,
    boot_hart: usize,
) -> ! {
    allocator::initialize();
    let words = DynamicWords {
        magic,
        version,
        next_address,
        next_mode,
        options,
        boot_hart,
    };
    for (slot, word) in
        DYNAMIC_SNAPSHOT
            .iter()
            .zip([magic, version, next_address, next_mode, options, boot_hart])
    {
        slot.store(word, Ordering::Relaxed);
    }
    // SAFETY: raw entry copied the dynamic words and established unique cold
    // initialization authority before this bounded DTB import.
    match unsafe {
        crate::boot::import::prepare_dynamic_boot(
            words,
            contract::selected_dtb(dtb_address),
            hart_id,
        )
    } {
        Ok(boot) => runtime::enter_policy(boot),
        Err(_) => runtime::fail_stop(),
    }
}
