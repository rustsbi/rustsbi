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
mod wakeup;

use alloc::boxed::Box;
use runtime::{
    Error, PlatformView, Result, SpacemitK1Registers, hart::HartId, memory::MemoryRegistry,
};

use runtime::hart::HartWake;

use cci::Cci550;

use reset_vector::ResetVectorRegisters;

/// MMIO resources used by the K1 cold-boot sequence.
pub(crate) struct K1BootResources {
    system_registers: SpacemitK1Registers,
    reset_vectors: ResetVectorRegisters,
    cci: Cci550<2>,
    wakeup: wakeup::K1Wakeup,
}

impl K1BootResources {
    pub(crate) fn acquire(
        memory: &mut MemoryRegistry,
        registers: SpacemitK1Registers,
    ) -> Result<Self> {
        Ok(Self {
            system_registers: registers,
            reset_vectors: ResetVectorRegisters::acquire(memory, registers.reset_vectors())?,
            cci: Cci550::acquire(
                memory,
                registers.cci_status(),
                registers.cci_snoop_controls(),
            )?,
            wakeup: wakeup::K1Wakeup::acquire(memory, registers)?,
        })
    }
}

/// Performs the K1 per-hart L2 and machine-feature setup.
pub(crate) fn initialize_hart(registers: SpacemitK1Registers) {
    registers.enable_hart_l2(
        HartId::current()
            .expect("BUG: current hart exceeds Runtime capacity")
            .as_usize(),
    );
    registers.enable_machine_features();
}

/// Runs the K1-only setup performed once by the boot hart.
pub(crate) fn initialize_boot_hart(resources: K1BootResources) -> impl HartWake {
    initialize_hart(resources.system_registers);
    resources
        .reset_vectors
        .set_reset_vector(crate::firmware::warm_entry as *const () as usize as u64);
    resources.cci.enable_coherency();
    resources.wakeup
}

/// SpacemiT K1 platform driver, selected by the K1 fixed-register capability.
pub(crate) struct K1Driver;

pub(crate) static K1: K1Driver = K1Driver;

impl crate::platform::PlatformDriver for K1Driver {
    fn matches(&self, platform: &PlatformView<'_>) -> Result<bool> {
        Ok(platform.spacemit_k1_registers()?.is_some())
    }

    fn create(&self) -> Box<dyn crate::platform::Platform> {
        Box::new(K1Platform::default())
    }
}

#[derive(Default)]
struct K1Platform {
    resources: Option<K1BootResources>,
}

impl crate::platform::Platform for K1Platform {
    fn early_init(
        &mut self,
        _cold_boot: bool,
        platform: &PlatformView<'_>,
        _info: &mut crate::info::PlatformInfo,
        memory: &mut MemoryRegistry,
    ) -> Result<()> {
        let registers = platform
            .spacemit_k1_registers()?
            .ok_or(Error::InvalidArgs)?;
        self.resources = Some(K1BootResources::acquire(memory, registers)?);
        Ok(())
    }

    fn final_init(
        &mut self,
        _cold_boot: bool,
        _platform: &PlatformView<'_>,
        _info: &mut crate::info::PlatformInfo,
        _memory: &mut MemoryRegistry,
    ) -> Result<()> {
        if let Some(resources) = self.resources.take() {
            let registers = resources.system_registers;
            crate::services::set_secondary_init(Box::new(move || initialize_hart(registers)));
            let wake: Box<dyn HartWake> = Box::new(initialize_boot_hart(resources));
            let wake: &'static dyn HartWake = Box::leak(wake);
            runtime::hart::install_wakeup(Some(wake));
        }
        Ok(())
    }
}
