#![forbid(unsafe_code)]

//! Board selection and shared platform initialization.
//!
//! The boot hart reads the device tree and publishes the initialized services.

pub(crate) mod allwinner;
mod boot;
mod harts;
mod interrupts;
mod report;
pub(crate) mod spacemit_k1;

pub(crate) mod qemu_aplic;

use runtime::{
    PlatformDescription, PlatformView, Result,
    memory::{MemoryRegistry, SupervisorMemory},
    soc::{
        allwinner::{v821::AllwinnerV821Soc, v861::AllwinnerV861Soc},
        spacemit::k1::SpacemitK1Soc,
    },
};

use crate::sbi::{pmu::SbiPmu, vendor::Vendor};

pub use boot::wait_until_ready;
pub(crate) use spacemit_k1::initialize_secondary_hart;

trait Board {
    /// Prepares the board before device binding and returns any alias offset for PMP.
    fn early_init(
        &self,
        _memory: &mut MemoryRegistry,
        _timer: Option<&interrupts::TimerDesc>,
    ) -> Result<Option<u64>> {
        Ok(None)
    }

    /// Initializes board devices after the shared devices are ready.
    fn init(
        &self,
        _platform: &PlatformView<'_>,
        _memory: &mut MemoryRegistry,
        _supervisor_memory: &'static SupervisorMemory,
        _pmu: &mut Option<SbiPmu>,
    ) -> Result<Option<Vendor>> {
        Ok(None)
    }
}

struct Generic;

impl Board for Generic {}

/// Initializes the platform on the boot hart and returns the next-stage FDT address.
pub fn init_platform(description: PlatformDescription) -> Result<usize> {
    if let Some(board) = description.inspect(|view| view.soc::<AllwinnerV821Soc>())? {
        return boot::init(description, &board);
    }
    if let Some(board) = description.inspect(|view| view.soc::<SpacemitK1Soc>())? {
        return boot::init(description, &board);
    }
    if let Some(board) = description.inspect(|view| view.soc::<AllwinnerV861Soc>())? {
        return boot::init(description, &board);
    }
    boot::init(description, &Generic)
}
