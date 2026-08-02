//! Previous-firmware dynamic handoff.

use core::sync::atomic::{AtomicUsize, Ordering};

use crate::config::BOOT_STACK_SIZE;

use super::stacks::{BOOT_STACK, HART_STACKS, HartStack};
use super::{
    EARLY_FAILED, EARLY_INITIALIZING, EARLY_READY, EARLY_STATE, RUNTIME_READY, RUNTIME_STATE,
};
use super::{allocator, enter_policy, fail_stop, image, relocation, warm_entry};

const MSTATUS_MPRV: usize = 1 << 17;
const DYNAMIC_WORD_COUNT: usize = 6;
const DYNAMIC_MAGIC: usize = 0x4942_534f;

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
        include_str!("dynamic.S"),
        mprv = const MSTATUS_MPRV,
        xlen_align_mask = const core::mem::size_of::<usize>() - 1,
        dynamic_size = const DYNAMIC_WORD_COUNT * core::mem::size_of::<usize>(),
        word_size = const core::mem::size_of::<usize>(),
        dynamic_magic = const DYNAMIC_MAGIC,
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
    for (slot, word) in
        DYNAMIC_SNAPSHOT
            .iter()
            .zip([magic, version, next_address, next_mode, options, boot_hart])
    {
        slot.store(word, Ordering::Relaxed);
    }
    if magic != DYNAMIC_MAGIC
        || !matches!(version, 1 | 2)
        || !image::next_address_allowed(next_address)
    {
        fail_stop();
    }
    let mode = match next_mode {
        0 => crate::boot::NextMode::User,
        1 => crate::boot::NextMode::Supervisor,
        3 => crate::boot::NextMode::Machine,
        _ => fail_stop(),
    };
    let _ = (options, boot_hart);
    // SAFETY: raw entry established unique cold initialization authority and
    // the provider DTB remains readable through this bounded copy.
    let dtb = match unsafe { crate::boot::copy_from_entry(image::selected_dtb(dtb_address)) } {
        Ok(dtb) => dtb,
        Err(_) => fail_stop(),
    };
    enter_policy(crate::boot::BootInfo::new(
        dtb,
        crate::boot::NextStage::new(next_address, 0, mode),
        hart_id,
    ))
}
