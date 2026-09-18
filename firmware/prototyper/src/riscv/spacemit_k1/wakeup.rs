//! K1 PMU secondary-hart wakeup.
//!
//! K1 User Manual sections 9.9.4.9.3 and 9.9.4.9.12 define CORE_STATUS
//! and the per-caller wakeup registers. The sequence follows the vendor
//! [OpenSBI implementation].
//!
//! [OpenSBI implementation]: https://github.com/spacemit-com/opensbi/blob/fc02b891b17b8bdc1273a39f80aa374cd99ba9a2/lib/utils/psci/spacemit/plat/k1x/underly_implement.c

use runtime::memory::{MemoryRegistry, MmioRegion};

use crate::driver::HartWake;
use crate::driver::spacemit_k1_syscon_apmu;
use runtime::hart::HartId;

/// These registers live inside the APMU window. When the device tree
/// describes the APMU peripheral it owns that window and the wake sequence
/// goes through it; otherwise the fixed register ranges are acquired here.
pub(super) enum K1Wakeup {
    Legacy {
        status: MmioRegion,
        controls: [MmioRegion; 2],
    },
    Apmu,
}

impl K1Wakeup {
    pub(super) fn acquire(
        memory: &mut MemoryRegistry,
        registers: runtime::SpacemitK1Registers,
        apmu_available: bool,
    ) -> runtime::Result<Self> {
        if apmu_available {
            return Ok(Self::Apmu);
        }
        let [cluster0, cluster1] = registers.wakeup_controls();
        Ok(Self::Legacy {
            status: memory.acquire_mmio(registers.core_status())?,
            controls: [
                memory.acquire_mmio(cluster0)?,
                memory.acquire_mmio(cluster1)?,
            ],
        })
    }

    fn core_status(&self) -> runtime::Result<u32> {
        match self {
            Self::Legacy { status, .. } => status.read::<u32>(0),
            Self::Apmu => spacemit_k1_syscon_apmu::get()
                .ok_or(runtime::Error::NotEnoughResources)?
                .core_status(),
        }
    }

    fn request_wakeup(&self, caller: usize, request: u32) -> runtime::Result<()> {
        match self {
            Self::Legacy { controls, .. } => {
                controls[caller / 4].write((caller % 4) * size_of::<u32>(), request)
            }
            Self::Apmu => spacemit_k1_syscon_apmu::get()
                .ok_or(runtime::Error::NotEnoughResources)?
                .request_wakeup(caller, request),
        }
    }
}

impl HartWake for K1Wakeup {
    fn wake(&self, hart: HartId) -> runtime::Result<bool> {
        let hart_id = hart.as_usize();
        let caller = HartId::current()
            .map_err(|_| runtime::Error::InvalidArgs)?
            .as_usize();
        if hart_id >= 8 || caller >= 8 {
            return Err(runtime::Error::InvalidArgs);
        }
        let c2_bit = 6 + (hart_id / 4) * 16 + (hart_id % 4) * 3;
        if self.core_status()? & (1 << c2_bit) == 0 {
            return Ok(false);
        }

        // Order firmware state and the reset vector before the wake request.
        riscv::asm::fence();
        // Each caller writes its own register. Bits are wake requests,
        // cleared by hardware; writing zero has no effect, so never RMW.
        self.request_wakeup(caller, 1u32 << hart_id)?;
        riscv::asm::fence();
        Ok(true)
    }
}
