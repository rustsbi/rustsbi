//! Compatibility reference: K1 reset-vector registers from the pinned
//! [OpenSBI K1 platform header].
//!
//! [OpenSBI K1 platform header]: https://github.com/riscv-software-src/opensbi/blob/35511bc6ee1c9c17b6a89b44c52e2044bb51b979/platform/generic/include/spacemit/k1.h

use runtime::memory::{DeviceRegisterRange, MemoryRegistry, MmioRegion};

#[repr(usize)]
enum Register {
    AddressLow = 0x00,
    AddressHigh = 0x04,
}

pub(super) struct ResetVectorRegisters([MmioRegion; 2]);

impl ResetVectorRegisters {
    pub(super) fn acquire(
        memory: &mut MemoryRegistry,
        [cluster0, cluster1]: [DeviceRegisterRange; 2],
    ) -> runtime::Result<Self> {
        let cluster0 = memory.acquire_mmio(cluster0)?;
        let cluster1 = memory.acquire_mmio(cluster1)?;
        Ok(Self([cluster0, cluster1]))
    }

    pub(super) fn set_reset_vector(&self, address: u64) {
        for registers in &self.0 {
            registers
                .write(Register::AddressLow as usize, address as u32)
                .expect("BUG: K1 reset-vector low register escaped its MMIO window");
            registers
                .write(
                    Register::AddressHigh as usize,
                    (address >> u32::BITS) as u32,
                )
                .expect("BUG: K1 reset-vector high register escaped its MMIO window");
        }
    }
}
