//! Linker-bounded payload and device-tree image storage.
//!
//! These sections are writable handoff storage rather than Rust statics.
//! Runtime returns physical ranges, without creating Rust references to bytes
//! that the next stage may overwrite.

#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
use crate::memory::PhysAddr;
use crate::memory::PhysAddrRange;

#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
fn from_linker_bounds(start: usize, end: usize, alignment: usize) -> Option<PhysAddrRange> {
    assert!(
        start.is_multiple_of(alignment),
        "BUG: embedded image linker address is misaligned"
    );
    if start == end {
        return None;
    }
    Some(
        PhysAddrRange::new(PhysAddr::new(start), PhysAddr::new(end))
            .expect("BUG: embedded image linker bounds are reversed"),
    )
}

/// Returns the payload section, or `None` when no payload was linked.
///
/// The linker must define `sbi_payload_start` and `sbi_payload_end` around
/// exactly the `.payload` input bytes, with the start aligned to four bytes.
pub fn embedded_payload() -> Option<PhysAddrRange> {
    #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
    {
        let start: usize;
        let end: usize;
        // SAFETY: these fixed linker symbols bound a dedicated image section.
        // Loading their relocated addresses neither accesses the bytes nor
        // constructs references that could conflict with next-stage writes.
        unsafe {
            core::arch::asm!(
                "lla {start}, sbi_payload_start",
                "lla {end}, sbi_payload_end",
                start = out(reg) start,
                end = out(reg) end,
                options(nomem, nostack, preserves_flags),
            );
        }
        from_linker_bounds(start, end, 4)
    }
    #[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
    unimplemented!("Embedded-image addresses require a RISC-V target")
}

/// Returns the FDT section, or `None` when no FDT was linked.
///
/// The linker must define `sbi_fdt_start` and `sbi_fdt_end` around exactly
/// the `.fdt` input bytes, with the start aligned to sixteen bytes.
pub fn embedded_fdt() -> Option<PhysAddrRange> {
    #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
    {
        let start: usize;
        let end: usize;
        // SAFETY: the fixed linker symbols delimit the FDT handoff storage.
        // Only relocated addresses escape; Runtime creates no byte references.
        unsafe {
            core::arch::asm!(
                "lla {start}, sbi_fdt_start",
                "lla {end}, sbi_fdt_end",
                start = out(reg) start,
                end = out(reg) end,
                options(nomem, nostack, preserves_flags),
            );
        }
        from_linker_bounds(start, end, 16)
    }
    #[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
    unimplemented!("Embedded-image addresses require a RISC-V target")
}
