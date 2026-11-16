//! Raw access to the boot handoff supplied by the previous stage.
//!
//! The remaining non-volatile DTB source operations here apply only to the
//! early phase before [`crate::boot::prepare_traps`] installs trap state.
//! They do not cover `SupervisorMemory`, arbitrary physical memory, or any
//! access after supervisor or virtual-supervisor execution begins.
//!
//! # References
//!
//! - Compatibility reference: [OpenSBI `fw_dynamic` interface](https://github.com/riscv-software-src/opensbi/blob/019a8e69a1dc0c0f011fabd0372e1ba80e40dd7c/include/sbi/fw_dynamic.h) —
//!   `DynamicInfo` layout, magic value, versions, and boot-hart encoding.

use fdt::Fdt;

use crate::memory::PhysAddr;
use crate::{Error, Result};

/// M-mode firmware dynamic information.
#[derive(Clone, Copy)]
#[repr(C)]
pub struct DynamicInfo {
    /// Dynamic information magic value.
    pub magic: usize,
    /// Version of dynamic information.
    pub version: usize,
    /// Address of the next boot-loading stage.
    pub next_addr: usize,
    /// RISC-V privilege mode of the next boot-loading stage.
    pub next_mode: usize,
    /// M-mode firmware options; its definition varies between SBI implementations.
    pub options: usize,
    /// Boot hart ID of current environment.
    pub boot_hart: usize,
}

/// Returns the complete FDT byte range described by an entry-point address.
///
/// # Safety
///
/// Call only before [`crate::boot::prepare_traps`] installs trap state. The
/// caller must ensure that `address` is non-null and that the fixed FDT
/// header plus the complete span declared by its `totalsize` field are
/// readable and unchanged for the returned slice's lifetime.
pub unsafe fn fdt_source<'a>(address: PhysAddr) -> Result<&'a [u8]> {
    if address.as_usize() == 0 {
        return Err(Error::InvalidArgs);
    }
    // SAFETY: the caller guarantees that the address points to a readable FDT
    // header and complete declared span.
    let fdt = unsafe { Fdt::from_ptr(address.as_usize() as *const u8) }
        .map_err(|_| Error::InvalidArgs)?;
    // SAFETY: the same caller guarantee covers the complete `totalsize` span.
    Ok(unsafe { core::slice::from_raw_parts(address.as_usize() as *const u8, fdt.total_size()) })
}

/// Reads the `DynamicInfo` record at an entry-point address.
///
/// # Safety
///
/// `address` must reference a `DynamicInfo`-aligned, initialized record in
/// ordinary readable RAM that stays stable for the duration of the read and
/// cannot overlap any Rust allocation.
pub unsafe fn read_dynamic_info(address: usize) -> DynamicInfo {
    // SAFETY: the caller guarantees an aligned, initialized record in stable
    // ordinary RAM outside every Rust allocation.
    unsafe { core::ptr::read_volatile(address as *const DynamicInfo) }
}

/// Writes `source` to the next-stage DTB destination and zeroes the padding.
///
/// The write is volatile because the destination lies outside every Rust
/// allocation. Byte order and `totalsize` handling belong to the caller.
///
/// # Safety
///
/// - `destination` must reference exclusive, writable ordinary RAM of at
///   least `capacity` bytes, live for the write.
/// - The destination span must not overlap any Rust allocation.
/// - `source.len() <= capacity` and the whole `destination + capacity` span
///   must be free of address overflow.
pub unsafe fn write_dtb(destination: PhysAddr, source: &[u8], capacity: usize) {
    let base = destination.as_usize();
    for (offset, byte) in source.iter().enumerate() {
        // SAFETY: the caller guarantees the exclusive destination span covers
        // every `base + offset` below `capacity`.
        unsafe { ((base + offset) as *mut u8).write_volatile(*byte) };
    }
    for offset in source.len()..capacity {
        // SAFETY: the same exclusive destination span covers the padding.
        unsafe { ((base + offset) as *mut u8).write_volatile(0) };
    }
}
