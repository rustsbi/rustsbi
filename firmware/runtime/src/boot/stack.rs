//! Two-stage stack setup for the enabled hart topology.
//!
//! Entry serializes boot candidates on one linker-owned stack. The selected
//! boot hart keeps it; the other harts receive stacks immediately after the
//! linked image. Entry releases them only after discovery has left Rust.

use alloc::vec::Vec;
use core::cell::UnsafeCell;
use core::mem::MaybeUninit;

use spin::Once;

use crate::hart::{HART_TABLE, HartEntry};
use crate::memory::{PhysAddr, PhysAddrRange, linker_image_bounds};
use crate::trap::entry::FRAME_BYTES;
use crate::{Error, PlatformDescription, Result};

static FIRMWARE_END: Once<usize> = Once::new();

/// Linker-owned storage used first for discovery, then by the boot hart.
///
/// No safe method exposes its bytes. Entry serializes discovery and later
/// gives this stack exclusively to the selected boot hart.
#[repr(C, align(128))]
pub struct BootStack<const SIZE: usize>(UnsafeCell<MaybeUninit<[u8; SIZE]>>);

// SAFETY: only entry assembly accesses the bytes, under the bootstrap lock
// until ownership passes permanently to the selected boot hart.
unsafe impl<const SIZE: usize> Sync for BootStack<SIZE> {}

impl<const SIZE: usize> BootStack<SIZE> {
    /// Creates uninitialized stack storage for a firmware static.
    pub const fn uninit() -> Self {
        assert!(
            SIZE.is_multiple_of(core::mem::align_of::<Self>()),
            "stack size must be a multiple of the stack alignment"
        );
        assert!(SIZE > FRAME_BYTES, "stack size must exceed the trap frame");
        Self(UnsafeCell::new(MaybeUninit::uninit()))
    }
}

pub(crate) fn firmware_end() -> Option<usize> {
    FIRMWARE_END.get().copied()
}

/// Initializes the shared hart topology and its stacks before policy boot.
///
/// The heap must already be initialized. `capacity` limits the enabled hart
/// count, not the numerical values of hardware IDs.
///
/// # Safety
///
/// Called once by the selected boot hart while it exclusively owns `boot_stack`
/// and other harts wait without stacks. The loader must provide exclusive RAM
/// after the linked image for the remaining stacks, including exclusion of the
/// complete next-stage image and all other live loader objects. The checks here
/// cover described RAM, reservations, the DTB, handoff and next-stage entry;
/// the entry address alone cannot describe the entire next-stage image.
/// Entry must return from this Rust call before releasing secondary harts.
#[doc(hidden)]
pub unsafe fn initialize_stacks<const SIZE: usize>(
    boot_stack: &'static BootStack<SIZE>,
    capacity: usize,
    platform: &PlatformDescription,
    next_stage_entry: usize,
    handoff: Option<PhysAddrRange>,
) -> Result<()> {
    if FIRMWARE_END.is_completed() {
        return Err(Error::InvalidArgs);
    }
    let mut entries = platform.inspect(|view| {
        let mut entries = Vec::new();
        for raw_id in view.hart_ids()? {
            if entries.len() == capacity {
                return Err(Error::NotEnoughResources);
            }
            entries.push(HartEntry {
                raw_id: raw_id?,
                stack_top: 0,
            });
        }
        Ok(entries)
    })?;
    entries.sort_unstable_by_key(|entry| entry.raw_id);
    if entries.is_empty()
        || entries
            .windows(2)
            .any(|pair| pair[0].raw_id == pair[1].raw_id)
    {
        return Err(Error::InvalidArgs);
    }
    let boot_id = crate::csr::mhartid();
    let boot_index = entries
        .binary_search_by_key(&boot_id, |entry| entry.raw_id)
        .map_err(|_| Error::InvalidArgs)?;
    let (image_start, image_end) = linker_image_bounds()?;
    let boot_start = boot_stack.0.get() as usize;
    let boot_top = boot_start.checked_add(SIZE).ok_or(Error::Overflow)?;
    if boot_start < image_start
        || boot_top > image_end
        || !image_end.is_multiple_of(core::mem::align_of::<BootStack<SIZE>>())
    {
        return Err(Error::InvalidArgs);
    }
    let extra_size = (entries.len() - 1)
        .checked_mul(SIZE)
        .ok_or(Error::Overflow)?;
    let end = image_end.checked_add(extra_size).ok_or(Error::Overflow)?;
    let firmware = PhysAddrRange::new(PhysAddr::new(image_start), PhysAddr::new(end))?;
    if extra_size != 0 {
        let stacks = PhysAddrRange::new(PhysAddr::new(image_end), PhysAddr::new(end))?;
        let dtb = platform.inspect(|view| Ok(view.storage_range()))?;
        if stacks.overlaps(dtb)
            || (image_end..end).contains(&next_stage_entry)
            || handoff.is_some_and(|range| stacks.overlaps(range))
        {
            return Err(Error::AccessDenied);
        }
        let (_, memory) = platform.memory_resources()?;
        memory.validate_firmware_extension(firmware)?;
    }
    let mut top = image_end;
    for (index, entry) in entries.iter_mut().enumerate() {
        entry.stack_top = if index == boot_index {
            boot_top
        } else {
            top += SIZE;
            top
        };
    }
    FIRMWARE_END.call_once(|| end);
    crate::hart::publish(entries.into_boxed_slice());
    Ok(())
}

/// Selects the current hart's initialized stack without using a stack.
///
/// # Safety
///
/// Entry/exit helper only. The caller must be ready to discard its old call
/// chain. Cold entry must wait until the bootstrap owner has left Rust.
#[cfg_attr(any(target_arch = "riscv32", target_arch = "riscv64"), unsafe(naked))]
pub unsafe extern "C" fn locate_stack() {
    #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
    core::arch::naked_asm!(
        "csrr t1, mhartid",
        "la t0, {table}",
        ".if {xlen} == 64",
        "ld t2, 0(t0)",
        ".else",
        "lw t2, 0(t0)",
        ".endif",
        "fence r, rw",
        "beqz t2, {fail}",
        ".if {xlen} == 64",
        "ld t0, 8(t0)",
        ".else",
        "lw t0, 4(t0)",
        ".endif",
        "1:",
        "beqz t0, {fail}",
        ".if {xlen} == 64",
        "ld t3, 0(t2)",
        ".else",
        "lw t3, 0(t2)",
        ".endif",
        "beq t1, t3, 2f",
        "addi t2, t2, {entry_size}",
        "addi t0, t0, -1",
        "j 1b",
        "2:",
        ".if {xlen} == 64",
        "ld sp, 8(t2)",
        ".else",
        "lw sp, 4(t2)",
        ".endif",
        "ret",
        xlen = const usize::BITS,
        entry_size = const size_of::<HartEntry>(),
        table = sym HART_TABLE,
        fail = sym super::fail_stop,
    );
    #[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
    unimplemented!("Stack selection requires a RISC-V target");
}
