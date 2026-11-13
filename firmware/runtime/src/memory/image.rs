//! Physical bounds of the current firmware image.

use super::{PhysAddr, PhysAddrRange};
#[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
use crate::Error;
use crate::Result;

/// Returns firmware-owned memory, including the stacks established at boot.
pub fn locate_firmware_image() -> Result<PhysAddrRange> {
    let (image_start, image_end) = linker_image_bounds()?;
    let image_end = crate::boot::firmware_end().unwrap_or(image_end);
    PhysAddrRange::new(PhysAddr::new(image_start), PhysAddr::new(image_end))
}

/// Returns the embedded payload storage linked outside the firmware image,
/// or `None` when this build embeds no payload.
///
/// The linker defines the payload bounds even for builds without a payload:
/// an empty section yields equal bounds, which this function reports as no
/// payload storage.
pub(crate) fn locate_payload_image() -> Result<Option<PhysAddrRange>> {
    let (start, end) = linker_payload_bounds()?;
    if start == end {
        return Ok(None);
    }
    PhysAddrRange::new(PhysAddr::new(start), PhysAddr::new(end)).map(Some)
}

fn linker_payload_bounds() -> Result<(usize, usize)> {
    match () {
        #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
        () => {
            let (start, end);
            // SAFETY: `payload_start` and `payload_end` are addresses supplied
            // by the firmware linker script. `lla` resolves those addresses
            // without dereferencing them.
            unsafe {
                core::arch::asm!(
                    "lla {start}, payload_start",
                    "lla {end}, payload_end",
                    start = out(reg) start,
                    end = out(reg) end,
                    options(nomem, nostack),
                );
            }
            Ok((start, end))
        }
        #[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
        () => Err(Error::NotEnoughResources),
    }
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
        () => Err(Error::NotEnoughResources),
    }
}
