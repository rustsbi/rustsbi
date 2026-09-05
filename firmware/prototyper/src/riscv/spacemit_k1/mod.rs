//! SpacemiT K1 processor setup.
//!
//! Compatibility reference: [OpenSBI's K1 platform header] defines the
//! private CSRs and SoC addresses, while its [K1 platform implementation]
//! supplies the startup sequence. Hardware manual: [Arm CoreLink CCI-550
//! TRM], chapter 3, defines the CCI register semantics.
//!
//! [OpenSBI's K1 platform header]: https://github.com/riscv-software-src/opensbi/blob/35511bc6ee1c9c17b6a89b44c52e2044bb51b979/platform/generic/include/spacemit/k1.h
//! [K1 platform implementation]: https://github.com/riscv-software-src/opensbi/blob/35511bc6ee1c9c17b6a89b44c52e2044bb51b979/platform/generic/spacemit/k1.c
//! [Arm CoreLink CCI-550 TRM]: https://documentation-service.arm.com/static/5e7dd450cbfe76649ba52b0c

#![forbid(unsafe_code)]

mod cci;
mod reset_vector;

use runtime::{SpacemitK1Registers, memory::MemoryRegistry};

use crate::riscv::current_hartid;

use cci::Cci;
use reset_vector::ResetVectorRegisters;

/// MMIO resources used by the K1 cold-boot sequence.
pub(crate) struct K1BootResources {
    system_registers: SpacemitK1Registers,
    reset_vectors: ResetVectorRegisters,
    cci: Cci,
}

impl K1BootResources {
    pub(crate) fn acquire(
        memory: &mut MemoryRegistry,
        registers: SpacemitK1Registers,
    ) -> runtime::Result<Self> {
        Ok(Self {
            system_registers: registers,
            reset_vectors: ResetVectorRegisters::acquire(memory, registers.reset_vectors())?,
            cci: Cci::acquire(
                memory,
                registers.cci_status(),
                registers.cci_snoop_controls(),
            )?,
        })
    }
}

/// Performs the K1 per-hart L2 setup.
pub(crate) fn initialize_hart(registers: SpacemitK1Registers) {
    registers.enable_hart_l2(current_hartid());
}

/// Runs the K1-only setup performed once by the boot hart.
pub(crate) fn initialize_boot_hart(resources: K1BootResources) {
    resources.system_registers.enable_hart_l2(current_hartid());
    resources.system_registers.enable_machine_features();
    resources
        .reset_vectors
        .set_reset_vector(crate::cfg::SBI_LINK_START_ADDRESS as u64);
    resources.cci.enable_coherency();
}
