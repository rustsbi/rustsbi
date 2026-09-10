//! K1 PMU secondary-hart wakeup.
//!
//! K1 User Manual sections 9.9.4.9.3 and 9.9.4.9.12 define CORE_STATUS
//! and the per-caller wakeup registers. The sequence follows the vendor
//! [OpenSBI implementation].
//!
//! [OpenSBI implementation]: https://github.com/spacemit-com/opensbi/blob/fc02b891b17b8bdc1273a39f80aa374cd99ba9a2/lib/utils/psci/spacemit/plat/k1x/underly_implement.c

use runtime::memory::{MemoryRegistry, MmioRegion};

use crate::driver::HartWake;
use crate::riscv::current_hartid;

pub(super) struct K1Wakeup {
    status: MmioRegion,
    controls: [MmioRegion; 2],
}

impl K1Wakeup {
    pub(super) fn acquire(
        memory: &mut MemoryRegistry,
        registers: runtime::SpacemitK1Registers,
    ) -> runtime::Result<Self> {
        let [cluster0, cluster1] = registers.wakeup_controls();
        Ok(Self {
            status: memory.acquire_mmio(registers.core_status())?,
            controls: [
                memory.acquire_mmio(cluster0)?,
                memory.acquire_mmio(cluster1)?,
            ],
        })
    }
}

impl HartWake for K1Wakeup {
    fn wake(&mut self, hart_id: usize) -> runtime::Result<bool> {
        let caller = current_hartid();
        if hart_id >= 8 || caller >= 8 {
            return Err(runtime::Error::InvalidArgs);
        }
        let c2_bit = 6 + (hart_id / 4) * 16 + (hart_id % 4) * 3;
        if self.status.read::<u32>(0)? & (1 << c2_bit) == 0 {
            return Ok(false);
        }

        // Order firmware state and the reset vector before the wake request.
        riscv::asm::fence();
        // Each caller writes its own register. Bits are wake requests,
        // cleared by hardware; writing zero has no effect, so never RMW.
        self.controls[caller / 4].write((caller % 4) * size_of::<u32>(), 1u32 << hart_id)?;
        riscv::asm::fence();
        Ok(true)
    }
}
