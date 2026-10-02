//! Physical bounds of the current firmware image.

use super::{PhysAddr, PhysAddrRange};
use crate::Result;

pub(crate) fn locate_firmware_image() -> Result<PhysAddrRange> {
    let (image_start, image_end) = linker_image_bounds()?;
    PhysAddrRange::new(PhysAddr::new(image_start), PhysAddr::new(image_end))
}

fn linker_image_bounds() -> Result<(usize, usize)> {
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
