//! Compatibility reference: K1 CCI base/interface mapping from the pinned
//! [OpenSBI K1 platform header].
//! Register fields and sequencing follow the [Arm CoreLink CCI-550 TRM],
//! sections 3.2, 3.3.3, and 3.3.10.
//!
//! [OpenSBI K1 platform header]: https://github.com/riscv-software-src/opensbi/blob/35511bc6ee1c9c17b6a89b44c52e2044bb51b979/platform/generic/include/spacemit/k1.h
//! [Arm CoreLink CCI-550 TRM]: https://documentation-service.arm.com/static/5e7dd450cbfe76649ba52b0c

use bitflags::bitflags;
use core::sync::atomic::{Ordering, fence};

use runtime::memory::{DeviceRegisterRange, MemoryRegistry, MmioRegion};

bitflags! {
    struct Status: u32 {
        const CHANGE_PENDING = 1 << 0;
    }

    struct SnoopControl: u32 {
        const ENABLE_SNOOPS = 1 << 0;
        const ENABLE_DVM_MESSAGES = 1 << 1;
    }
}

struct StatusRegister(MmioRegion);

impl StatusRegister {
    fn change_pending(&self) -> bool {
        Status::from_bits_retain(
            self.0
                .read(0)
                .expect("BUG: K1 CCI status register escaped its MMIO window"),
        )
        .contains(Status::CHANGE_PENDING)
    }
}

struct SnoopControlRegister(MmioRegion);

impl SnoopControlRegister {
    fn enable_coherency(&self) {
        self.0
            .write(
                0,
                (SnoopControl::ENABLE_SNOOPS | SnoopControl::ENABLE_DVM_MESSAGES).bits(),
            )
            .expect("BUG: K1 CCI snoop-control register escaped its MMIO window");
    }
}

pub(super) struct Cci {
    status: StatusRegister,
    snoop_control: [SnoopControlRegister; 2],
}

impl Cci {
    pub(super) fn acquire(
        memory: &mut MemoryRegistry,
        status: DeviceRegisterRange,
        [cluster0, cluster1]: [DeviceRegisterRange; 2],
    ) -> runtime::Result<Self> {
        let status = StatusRegister(memory.acquire_mmio(status)?);
        let cluster0 = SnoopControlRegister(memory.acquire_mmio(cluster0)?);
        let cluster1 = SnoopControlRegister(memory.acquire_mmio(cluster1)?);
        Ok(Self {
            status,
            snoop_control: [cluster0, cluster1],
        })
    }

    pub(super) fn enable_coherency(&self) {
        for control in &self.snoop_control {
            control.enable_coherency();
            fence(Ordering::SeqCst);
            while self.status.change_pending() {
                core::hint::spin_loop();
            }
        }
    }
}
