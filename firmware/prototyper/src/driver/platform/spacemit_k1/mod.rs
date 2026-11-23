//! SpacemiT K1 processor setup.
//!
//! [OpenSBI's K1 platform header] defines the private CSRs and SoC addresses,
//! while its [K1 platform implementation] supplies the startup sequence.
//! The [Arm CoreLink CCI-550 TRM], chapter 3, defines the CCI register semantics.
//!
//! [OpenSBI's K1 platform header]: https://github.com/riscv-software-src/opensbi/blob/35511bc6ee1c9c17b6a89b44c52e2044bb51b979/platform/generic/include/spacemit/k1.h
//! [K1 platform implementation]: https://github.com/riscv-software-src/opensbi/blob/35511bc6ee1c9c17b6a89b44c52e2044bb51b979/platform/generic/spacemit/k1.c
//! [Arm CoreLink CCI-550 TRM]: https://documentation-service.arm.com/static/5e7dd450cbfe76649ba52b0c

mod reset_vector;
mod wakeup;

use runtime::{
    PlatformView, Result,
    hart::install_wakeup,
    memory::{MemoryRegistry, SupervisorMemory},
    soc::spacemit::k1::SpacemitK1Soc,
};
use spin::Once;

use super::Board;
use crate::{
    driver::device::cci,
    firmware::k1_reset_entry,
    sbi::{pmu::SbiPmu, vendor::Vendor},
};

use reset_vector::ResetVectorRegisters;

static INITIALIZED: Once<SpacemitK1Soc> = Once::new();

impl Board for SpacemitK1Soc {
    fn init(
        &self,
        _platform: &PlatformView<'_>,
        memory: &mut MemoryRegistry,
        _supervisor_memory: &'static SupervisorMemory,
        _pmu: &mut Option<SbiPmu>,
    ) -> Result<Option<Vendor>> {
        // A failed MMIO claim must leave the K1 hardware state untouched.
        let wakeup = wakeup::K1Wakeup::init(self.apmu_registers()?, memory)?;
        let reset_vectors = ResetVectorRegisters::acquire(memory, self.ciu_registers()?)?;
        let cci = cci::Cci550::init(
            cci::Desc {
                registers: self.cci_registers()?,
                interfaces: [0, 1],
            },
            memory,
        )?;

        self.initialize_hart();
        reset_vectors.set_reset_vector(k1_reset_entry as *const () as usize as u64)?;
        cci.enable_coherency()?;
        INITIALIZED.call_once(|| *self);
        install_wakeup(wakeup);
        Ok(None)
    }
}

/// Configures the current secondary hart if the platform is K1.
///
/// Called in M-mode after the boot hart has published the platform.
pub(crate) fn initialize_secondary_hart() {
    if let Some(soc) = INITIALIZED.get() {
        soc.initialize_hart();
    }
}
