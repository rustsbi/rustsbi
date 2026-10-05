//! ACLINT MTIMER: per-hart MTIMECMP at +8.
//!
//! Only `mtimecmp` is bound; time comes from the `time` CSR.
//!
//! # References
//!
//! - Specification: [RISC-V ACLINT 1.0-rc4](https://github.com/riscvarchive/riscv-aclint/blob/4e570bfd3201f2c09e5afd290b5091526b0f099a/riscv-aclint.adoc) —
//!   "Machine-level Timer Device (MTIMER)": a separate MTIME base plus one
//!   64-bit MTIMECMP register per hart at the compare base.
//! - Specification: [RISC-V Privileged Architecture](https://github.com/riscv/riscv-isa-manual/blob/51c1291fc8168bf36530de3386d3f452069ce327/src/priv/machine.adoc) —
//!   "Machine Timer (`mtime` and `mtimecmp`) Registers": RV32 writes update one
//!   32-bit half at a time, and the sample sequence that avoids a spurious
//!   interrupt is exactly what `set_timer` performs.
//! - Reference implementation: [QEMU `create_fdt_socket_mtimer`](https://github.com/qemu/qemu/blob/v10.2.1/hw/riscv/virt.c#L409-L417) —
//!   the legacy unlabelled MTIMER node emitted by the `virt` machine.

use crate::{cfg::NUM_HART_MAX, driver::TimerBackend, platform::HartIndexMap};
use runtime::memory::{DeviceRegisterRange, MemoryRegistry, MmioRegion};

#[repr(usize)]
#[derive(Clone, Copy)]
enum Register {
    CompareLow = 0x00,
    CompareHigh = 0x04,
}

const COMPARE_STRIDE: usize = 8;

/// Byte offsets of the counter inside an MTIME window.
const MTIME_LOW_OFFSET: usize = 0x00;
const MTIME_HIGH_OFFSET: usize = 0x04;

pub(in crate::driver) struct Mtimer {
    compare: MmioRegion,
    time: Option<MmioRegion>,
    hart_indices: HartIndexMap,
}

pub(in crate::driver) fn bind(
    compare: DeviceRegisterRange,
    time: Option<DeviceRegisterRange>,
    memory: &mut MemoryRegistry,
    hart_indices: HartIndexMap,
) -> runtime::Result<Mtimer> {
    let hart_count =
        usize::try_from(hart_indices.count()).map_err(|_| runtime::Error::InvalidArgs)?;
    if hart_count == 0 || hart_count > NUM_HART_MAX {
        return Err(runtime::Error::InvalidArgs);
    }
    let window = Register::CompareLow as usize + COMPARE_STRIDE * hart_count;
    let compare = compare.subrange(0, window)?;
    if !compare.has_aligned_bounds(COMPARE_STRIDE) {
        return Err(runtime::Error::InvalidArgs);
    }
    let time = time.map(|time| memory.acquire_mmio(time)).transpose()?;
    Ok(Mtimer {
        compare: memory.acquire_mmio(compare)?,
        time,
        hart_indices,
    })
}

impl Mtimer {
    #[inline(always)]
    fn write_compare_word(&self, register: Register, hart_id: usize, value: u32) {
        // MTIMECMP registers are addressed by the index this device assigns to
        // the hart, which need not equal the hart ID.
        let index = self
            .hart_indices
            .get(hart_id)
            .expect("BUG: hart has no ACLINT MTIMECMP register") as usize;
        self.compare
            .write(register as usize + COMPARE_STRIDE * index, value.to_le())
            .expect("ACLINT MTIMER write outside acquired window");
    }

    /// Reads the free-running counter, retrying across a low-word wrap.
    fn read_mtime(&self) -> Option<u64> {
        let time = self.time.as_ref()?;
        loop {
            let high = read_word(time, MTIME_HIGH_OFFSET)?;
            let low = read_word(time, MTIME_LOW_OFFSET)?;
            if read_word(time, MTIME_HIGH_OFFSET)? == high {
                return Some((u64::from(high) << 32) | u64::from(low));
            }
        }
    }
}

fn read_word(time: &MmioRegion, offset: usize) -> Option<u32> {
    time.read::<u32>(offset).ok().map(u32::from_le)
}

impl TimerBackend for Mtimer {
    fn read_time(&self) -> Option<u64> {
        self.read_mtime()
    }

    fn read_time_low(&self) -> Option<usize> {
        #[cfg(target_pointer_width = "32")]
        {
            read_word(self.time.as_ref()?, MTIME_LOW_OFFSET).map(|value| value as usize)
        }
        #[cfg(target_pointer_width = "64")]
        {
            self.read_mtime().map(|value| value as usize)
        }
    }

    #[cfg(target_pointer_width = "32")]
    fn read_time_high(&self) -> Option<usize> {
        read_word(self.time.as_ref()?, MTIME_HIGH_OFFSET).map(|value| value as usize)
    }

    fn set_timer(&self, hart_id: usize, value: u64) {
        // Safe RV32 comparator update even if the old high half matches MTIME.
        self.write_compare_word(Register::CompareLow, hart_id, u32::MAX);
        self.write_compare_word(Register::CompareHigh, hart_id, (value >> 32) as u32);
        self.write_compare_word(Register::CompareLow, hart_id, value as u32);
        riscv::asm::fence();
    }
}
