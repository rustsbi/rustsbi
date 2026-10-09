//! Physical bounds of the current firmware image.

use super::{PhysAddr, PhysAddrRange};
use crate::Result;

/// Linker-owned image regions used when selecting the firmware protection plan.
///
/// The bounds are obtained without dereferencing linker symbols. Construction
/// checks that the read-only region lies entirely inside the firmware image.
pub struct FirmwareImageLayout {
    image: PhysAddrRange,
    read_only: PhysAddrRange,
}

impl FirmwareImageLayout {
    /// Returns the firmware image and any published secondary-stack storage.
    pub const fn image_range(&self) -> PhysAddrRange {
        self.image
    }

    /// Returns the range from `sbi_rodata_start` through `sbi_rodata_end`.
    pub const fn read_only_range(&self) -> PhysAddrRange {
        self.read_only
    }
}

/// Resolves and validates the protection boundaries of the current image.
pub fn firmware_image_layout() -> Result<FirmwareImageLayout> {
    #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
    {
        let image = locate_firmware_image()?;
        let (read_only_start, read_only_end);
        // SAFETY: PC-relative linker-symbol loads produce relocated addresses
        // in local registers; no section memory or mutable state is accessed.
        unsafe {
            core::arch::asm!(
                "lla {read_only_start}, sbi_rodata_start",
                "lla {read_only_end}, sbi_rodata_end",
                read_only_start = out(reg) read_only_start,
                read_only_end = out(reg) read_only_end,
                options(nomem, nostack),
            );
        }
        let read_only =
            PhysAddrRange::new(PhysAddr::new(read_only_start), PhysAddr::new(read_only_end))?;
        if !image.contains(read_only) {
            return Err(crate::Error::InvalidArgs);
        }
        Ok(FirmwareImageLayout { image, read_only })
    }
    #[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
    Err(crate::Error::NotEnoughResources)
}

pub(crate) fn locate_firmware_image() -> Result<PhysAddrRange> {
    let (image_start, image_end) = linker_image_bounds()?;
    let image_end = crate::boot::firmware_end().unwrap_or(image_end);
    PhysAddrRange::new(PhysAddr::new(image_start), PhysAddr::new(image_end))
}

pub(crate) fn linker_image_bounds() -> Result<(usize, usize)> {
    match () {
        #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
        () => {
            let (start, end);
            // SAFETY: `sbi_start` and `sbi_end` are addresses supplied by the firmware
            // linker script. `lla` resolves those addresses without dereferencing them.
            unsafe {
                core::arch::asm!(
                    "lla {start}, sbi_start",
                    "lla {end}, sbi_end",
                    start = out(reg) start,
                    end = out(reg) end,
                    options(nomem, nostack),
                );
            }
            Ok((start, end))
        }
        #[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
        () => Err(crate::Error::NotEnoughResources),
    }
}
