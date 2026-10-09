//! SpacemiT K1 processor setup.
//!
//! [OpenSBI's K1 platform header] defines the
//! private CSRs and SoC addresses, while its [K1 platform implementation]
//! supplies the startup sequence.
//! Hardware manual: [Arm CoreLink CCI-550
//! TRM], chapter 3, defines the CCI register semantics.
//!
//! [OpenSBI's K1 platform header]: https://github.com/riscv-software-src/opensbi/blob/35511bc6ee1c9c17b6a89b44c52e2044bb51b979/platform/generic/include/spacemit/k1.h
//! [K1 platform implementation]: https://github.com/riscv-software-src/opensbi/blob/35511bc6ee1c9c17b6a89b44c52e2044bb51b979/platform/generic/spacemit/k1.c
//! [Arm CoreLink CCI-550 TRM]: https://documentation-service.arm.com/static/5e7dd450cbfe76649ba52b0c

use runtime::{SpacemitK1Registers, memory::MemoryRegistry};

use crate::driver::Cci550;
use crate::driver::spacemit::k1::{K1HartWake, ResetVectorRegisters};
use runtime::hart::HartWakeDevice;

/// MMIO resources used by the K1 cold-boot sequence.
pub(crate) struct K1BootResources {
    system_registers: SpacemitK1Registers,
    reset_vectors: ResetVectorRegisters,
    cci: Cci550<2>,
    wakeup: K1HartWake,
}

impl K1BootResources {
    pub(crate) fn acquire(
        memory: &mut MemoryRegistry,
        registers: SpacemitK1Registers,
    ) -> runtime::Result<Self> {
        Ok(Self {
            system_registers: registers,
            reset_vectors: ResetVectorRegisters::acquire(memory, registers.reset_vectors())?,
            cci: Cci550::acquire(
                memory,
                registers.cci_status(),
                registers.cci_snoop_controls(),
            )?,
            wakeup: K1HartWake::acquire(memory, registers)?,
        })
    }
}

/// Runs the K1-only setup performed once by the boot hart.
pub(crate) fn initialize_boot_hart(resources: K1BootResources) -> impl HartWakeDevice {
    resources
        .system_registers
        .initialize_current_hart()
        .expect("BUG: current K1 hart exceeds Runtime capacity");
    resources.cci.enable_coherency();
    let entry = resources
        .system_registers
        .register_reset_entry(crate::boot::initialize_reset_hart)
        .expect("BUG: K1 reset entry registered more than once");
    resources.reset_vectors.set_reset_vector(entry.address());
    resources.wakeup
}
