//! Build-selected next-stage handoff.

use crate::config::BOOT_STACK_SIZE;

use super::stacks::{BOOT_STACK, HART_STACKS, HartStack};
use super::{
    EARLY_FAILED, EARLY_INITIALIZING, EARLY_READY, EARLY_STATE, RUNTIME_READY, RUNTIME_STATE,
};
use super::{allocator, enter_policy, fail_stop, image, relocation, warm_entry};

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
        initializing = const EARLY_INITIALIZING,
        ready = const EARLY_READY,
        early_state = sym EARLY_STATE,
        early_failed = sym EARLY_FAILED,
        runtime_state = sym RUNTIME_STATE,
        runtime_ready = const RUNTIME_READY,
        hart_entry_index = sym crate::hart::entry_index,
        hart_stacks = sym HART_STACKS,
        hart_stack_stride = const core::mem::size_of::<HartStack>(),
        warm_entry = sym warm_entry,
        relocation_update = sym relocation::relocation_update,
        boot_stack = sym BOOT_STACK,
        boot_stack_size = const BOOT_STACK_SIZE,
        cold_configured_entry = sym cold_entry,
    )
}

extern "C" fn cold_entry(hart_id: usize, provider_dtb_address: usize) -> ! {
    allocator::initialize();
    let Some((next_address, next_mode)) = image::fixed_stage() else {
        fail_stop()
    };
    // SAFETY: raw entry established unique cold initialization authority and
    // the provider DTB remains readable through this bounded copy.
    let dtb =
        match unsafe { crate::boot::copy_from_entry(image::selected_dtb(provider_dtb_address)) } {
            Ok(dtb) => dtb,
            Err(_) => fail_stop(),
        };
    enter_policy(crate::boot::BootInfo::new(
        dtb,
        crate::boot::NextStage::new(next_address, 0, next_mode),
        hart_id,
    ))
}
