//! SpacemiT K1 processor setup.
//!
//! [OpenSBI's K1 platform header] defines the private CSRs and SoC addresses,
//! while its [K1 platform implementation] supplies the startup sequence.
//! The [Arm CoreLink CCI-550 TRM], chapter 3, defines the CCI register semantics.
//!
//! [OpenSBI's K1 platform header]: https://github.com/riscv-software-src/opensbi/blob/35511bc6ee1c9c17b6a89b44c52e2044bb51b979/platform/generic/include/spacemit/k1.h
//! [K1 platform implementation]: https://github.com/riscv-software-src/opensbi/blob/35511bc6ee1c9c17b6a89b44c52e2044bb51b979/platform/generic/spacemit/k1.c
//! [Arm CoreLink CCI-550 TRM]: https://documentation-service.arm.com/static/5e7dd450cbfe76649ba52b0c

mod description;
mod reset_vector;
mod wakeup;

use alloc::boxed::Box;
use runtime::{
    PlatformView, Result,
    hart::{HartId, HartWake},
    memory::MemoryRegistry,
    soc::spacemit::k1::SpacemitK1Soc,
};
use spin::Once;

use crate::{driver::device::cci, firmware::k1_reset_entry};

pub(crate) use description::Desc;
use reset_vector::ResetVectorRegisters;

static INITIALIZED: Once<SpacemitK1Soc> = Once::new();

pub(crate) fn describe(platform: &PlatformView<'_>) -> Result<Option<Desc>> {
    Ok(platform.soc::<SpacemitK1Soc>()?.map(|soc| Desc { soc }))
}

/// Initializes K1 devices after acquiring their register windows.
pub(crate) fn init(description: Desc, memory: &mut MemoryRegistry) -> Result<Box<dyn HartWake>> {
    let soc = description.soc;
    // A failed MMIO claim must leave the K1 hardware state untouched.
    let wakeup = wakeup::K1Wakeup::init(soc.apmu_registers()?, memory)?;
    let reset_vectors = ResetVectorRegisters::acquire(memory, soc.ciu_registers()?)?;
    let cci = cci::Cci550::init(
        cci::Desc {
            registers: soc.cci_registers()?,
            interfaces: [0, 1],
        },
        memory,
    )?;

    soc.initialize_hart();
    reset_vectors.set_reset_vector(k1_reset_entry as *const () as usize as u64)?;
    cci.enable_coherency()?;
    INITIALIZED.call_once(|| soc);
    Ok(Box::new(wakeup))
}

/// Configures the current secondary hart if the platform is K1.
///
/// Called in M-mode after the boot hart has published the platform.
pub(crate) fn initialize_secondary_hart(_hart: HartId) {
    if let Some(soc) = INITIALIZED.get() {
        soc.initialize_hart();
    }
}
