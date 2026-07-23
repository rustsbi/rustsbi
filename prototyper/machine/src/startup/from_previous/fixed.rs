//! Build-selected next-stage handoff.

use crate::config::BOOT_STACK_SIZE;

use super::super::stacks::{BOOT_STACK, HART_STACKS, HartStack};
use super::super::{allocator, contract, relocation, runtime, state};

const MSTATUS_MPRV: usize = 1 << 17;

/// Raw entry for a build-selected next stage.
///
/// # Safety
///
/// The entry ABI supplies `a0 = hart_id` and a stable DTB address in `a1`.
#[unsafe(naked)]
pub(super) unsafe extern "C" fn entry() -> ! {
    core::arch::naked_asm!(
        include_str!("fixed.S"),
        mprv = const MSTATUS_MPRV,
        word_size = const core::mem::size_of::<usize>(),
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
        relocation_update = sym relocation::relocation_update,
        boot_stack = sym BOOT_STACK,
        boot_stack_size = const BOOT_STACK_SIZE,
        cold_configured_entry = sym cold_entry,
    )
}

extern "C" fn cold_entry(hart_id: usize, provider_dtb_address: usize) -> ! {
    allocator::initialize();
    let Some((next_address, next_mode)) = contract::fixed_stage() else {
        runtime::fail_stop()
    };
    // SAFETY: raw entry established unique cold initialization authority and
    // the selected DTB remains stable through this bounded import.
    match unsafe {
        crate::boot::import::prepare_fixed_boot(
            next_address,
            next_mode,
            contract::selected_dtb(provider_dtb_address),
            hart_id,
        )
    } {
        Ok(boot) => runtime::enter_policy(boot),
        Err(_) => runtime::fail_stop(),
    }
}
