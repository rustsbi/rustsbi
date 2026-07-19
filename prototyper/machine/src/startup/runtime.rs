//! Typed cold/warm Rust transfer and embedded next-stage inputs.

use core::sync::atomic::Ordering;

#[cfg(not(any(feature = "jump", feature = "payload")))]
use crate::boot::DynamicWords;
use crate::boot::{self, BootInfo};

#[cfg(not(any(feature = "jump", feature = "payload")))]
use super::state::DYNAMIC_SNAPSHOT;
use super::state::{
    EARLY_FAILED, EARLY_READY, EARLY_STATE, RUNTIME_FAILED, RUNTIME_READY, RUNTIME_STATE,
};

pub(super) extern "C" fn warm_entry(hart_id: usize, index: usize) -> ! {
    crate::boot::enter_warm_hart(hart_id, index)
}

pub(crate) fn publish_runtime() {
    RUNTIME_STATE.store(RUNTIME_READY, Ordering::Release);
}

pub(crate) fn fail_runtime() {
    RUNTIME_STATE.store(RUNTIME_FAILED, Ordering::Release);
}
#[cfg(not(any(feature = "jump", feature = "payload")))]
pub(super) extern "C" fn cold_entry(
    hart_id: usize,
    dtb_address: usize,
    magic: usize,
    version: usize,
    next_address: usize,
    next_mode: usize,
    options: usize,
    boot_hart: usize,
) -> ! {
    crate::memory::heap::initialize();
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

    // SAFETY: raw entry copied the dynamic words once, established unique
    // initialization authority, BSS, and a valid stack. The boot provider's
    // DTB envelope remains stable and readable until this bounded copy ends.
    match unsafe {
        boot::import::prepare_dynamic_boot(words, configured_dtb_address(dtb_address), hart_id)
    } {
        Ok(boot) => {
            EARLY_STATE.store(EARLY_READY, Ordering::Release);
            __rustsbi_prototyper_main(boot)
        }
        Err(_) => fail_stop(),
    }
}

#[cfg(any(feature = "jump", feature = "payload"))]
pub(super) extern "C" fn cold_fixed_entry(hart_id: usize, dtb_address: usize) -> ! {
    crate::memory::heap::initialize();
    let next_address = fixed_next_address();
    // SAFETY: raw entry established unique initialization authority, BSS, and
    // a valid stack. The selected DTB envelope remains stable and readable
    // until this bounded owned copy ends.
    match unsafe {
        boot::import::prepare_fixed_boot(next_address, configured_dtb_address(dtb_address), hart_id)
    } {
        Ok(boot) => {
            EARLY_STATE.store(EARLY_READY, Ordering::Release);
            __rustsbi_prototyper_main(boot)
        }
        Err(_) => fail_stop(),
    }
}

#[cfg(feature = "jump")]
fn fixed_next_address() -> usize {
    crate::config::FIXED_NEXT_ADDRESS
}

#[cfg(feature = "payload")]
fn fixed_next_address() -> usize {
    payload_image as *const () as usize
}

#[cfg(not(feature = "fdt"))]
fn configured_dtb_address(provider_address: usize) -> usize {
    provider_address
}

#[cfg(feature = "fdt")]
fn configured_dtb_address(_provider_address: usize) -> usize {
    embedded_dtb as *const () as usize
}

#[cfg(feature = "payload")]
/// Linker anchor for immutable payload bytes embedded by the build.
///
/// # Safety
///
/// This address-only symbol must never be called. The build must provide a
/// readable immutable payload file whose bytes remain part of this image.
#[unsafe(naked)]
#[unsafe(link_section = ".payload")]
unsafe extern "C" fn payload_image() -> ! {
    core::arch::naked_asm!(concat!(".incbin \"", env!("PROTOTYPER_PAYLOAD_PATH"), "\""))
}

#[cfg(feature = "fdt")]
/// Linker anchor for immutable device-tree bytes embedded by the build.
///
/// # Safety
///
/// This address-only symbol must never be called. The build must provide a
/// validated readable immutable DTB whose bytes remain part of this image.
#[unsafe(naked)]
#[unsafe(link_section = ".fdt")]
unsafe extern "C" fn embedded_dtb() -> ! {
    core::arch::naked_asm!(concat!(".incbin \"", env!("PROTOTYPER_FDT_PATH"), "\""))
}

unsafe extern "Rust" {
    safe fn __rustsbi_prototyper_main(boot: BootInfo) -> !;
}

fn fail_stop() -> ! {
    EARLY_FAILED.store(1, Ordering::Release);
    loop {
        // SAFETY: interrupts remain disabled and this terminal path owns no
        // live Rust borrow that could be observed after wakeup.
        unsafe { core::arch::asm!("wfi", options(nomem, nostack)) };
    }
}
