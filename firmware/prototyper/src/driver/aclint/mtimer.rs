//! ACLINT MTIMER: shared MTIME and per-hart MTIMECMP registers.
//!
//! # References
//!
//! - [RISC-V ACLINT 1.0], section 2, defines the MTIME and MTIMECMP register
//!   maps and the device-local HART index.
//! - RISC-V Privileged Architecture defines the MTIP interrupt and the RV32
//!   two-word access sequence used to update a 64-bit compare value safely.
//!
//! [RISC-V ACLINT 1.0]: https://github.com/riscvarchive/riscv-aclint/blob/main/riscv-aclint.adoc

use core::mem::size_of;
use runtime::{
    Error, Result,
    hart::HartId,
    memory::{MemoryRegistry, MmioRegion},
    timer::TimerDevice,
};

use crate::platform::AclintMtimer;

const COMPARE_STRIDE: usize = size_of::<u64>();

pub(super) struct Mtimer {
    compare: MmioRegion,
    time: Option<MmioRegion>,
    hart_indices: crate::platform::HartIndexMap,
}

pub(super) fn bind(resource: &AclintMtimer, memory: &mut MemoryRegistry) -> Result<Mtimer> {
    let hart_count = resource.hart_indices.count();
    if hart_count == 0 {
        return Err(Error::InvalidArgs);
    }
    let compare_size = hart_count
        .checked_mul(COMPARE_STRIDE)
        .ok_or(Error::Overflow)?;
    let compare = resource.compare.subrange(0, compare_size)?;
    if !compare.has_aligned_bounds(COMPARE_STRIDE) {
        return Err(Error::InvalidArgs);
    }

    let time = resource
        .time
        .map(|time| {
            let time = time.subrange(0, size_of::<u64>())?;
            if !time.has_aligned_bounds(size_of::<u64>()) {
                return Err(Error::InvalidArgs);
            }
            memory.acquire(time)
        })
        .transpose()?;
    let compare = memory.acquire(compare)?;
    Ok(Mtimer {
        compare,
        time,
        hart_indices: resource.hart_indices.clone(),
    })
}

impl Mtimer {
    fn compare_offset(&self, hart: HartId) -> usize {
        usize::try_from(
            self.hart_indices
                .get(hart)
                .expect("BUG: enabled hart has no ACLINT MTIMER slot"),
        )
        .expect("BUG: ACLINT MTIMER slot does not fit usize")
            * COMPARE_STRIDE
    }

    fn read_time_counter(&self) -> Option<u64> {
        let time = self.time.as_ref()?;
        #[cfg(target_pointer_width = "64")]
        {
            time.read::<u64>(0).ok().map(u64::from_le)
        }
        #[cfg(target_pointer_width = "32")]
        {
            loop {
                let high = u32::from_le(time.read::<u32>(size_of::<u32>()).ok()?);
                let low = u32::from_le(time.read::<u32>(0).ok()?);
                if u32::from_le(time.read::<u32>(size_of::<u32>()).ok()?) == high {
                    return Some((u64::from(high) << 32) | u64::from(low));
                }
            }
        }
    }
}

impl TimerDevice for Mtimer {
    fn read_time(&self) -> Option<u64> {
        self.read_time_counter()
    }

    fn read_time_low(&self) -> Option<usize> {
        #[cfg(target_pointer_width = "64")]
        {
            self.read_time_counter().map(|time| time as usize)
        }
        #[cfg(target_pointer_width = "32")]
        {
            self.time
                .as_ref()?
                .read::<u32>(0)
                .ok()
                .map(|value| u32::from_le(value) as usize)
        }
    }

    #[cfg(target_pointer_width = "32")]
    fn read_time_high(&self) -> Option<usize> {
        self.time
            .as_ref()?
            .read::<u32>(size_of::<u32>())
            .ok()
            .map(|value| u32::from_le(value) as usize)
    }

    fn set_compare(&self, hart: HartId, deadline: u64) {
        let offset = self.compare_offset(hart);
        #[cfg(target_pointer_width = "64")]
        self.compare
            .write(offset, deadline.to_le())
            .expect("BUG: ACLINT MTIMECMP write escaped its MMIO window");

        #[cfg(target_pointer_width = "32")]
        {
            // Suppress a transient comparison while replacing both halves.
            self.compare
                .write(offset, u32::MAX.to_le())
                .expect("BUG: ACLINT MTIMECMP low write escaped its MMIO window");
            self.compare
                .write(offset + size_of::<u32>(), ((deadline >> 32) as u32).to_le())
                .expect("BUG: ACLINT MTIMECMP high write escaped its MMIO window");
            self.compare
                .write(offset, (deadline as u32).to_le())
                .expect("BUG: ACLINT MTIMECMP low write escaped its MMIO window");
        }
    }
}
