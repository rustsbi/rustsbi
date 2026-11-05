//! SpacemiT K1 processor setup.
//!
//! [OpenSBI's K1 platform header] defines the private CSRs and SoC addresses,
//! while its [K1 platform implementation] supplies the startup sequence.
//! The [Arm CoreLink CCI-550 TRM], chapter 3, defines the CCI register semantics.
//!
//! [OpenSBI's K1 platform header]: https://github.com/riscv-software-src/opensbi/blob/35511bc6ee1c9c17b6a89b44c52e2044bb51b979/platform/generic/include/spacemit/k1.h
//! [K1 platform implementation]: https://github.com/riscv-software-src/opensbi/blob/35511bc6ee1c9c17b6a89b44c52e2044bb51b979/platform/generic/spacemit/k1.c
//! [Arm CoreLink CCI-550 TRM]: https://documentation-service.arm.com/static/5e7dd450cbfe76649ba52b0c

#![forbid(unsafe_code)]

mod description;
mod reset_vector;
mod wakeup;

use alloc::boxed::Box;
use runtime::{
    PlatformView, Result, SpacemitK1Registers,
    hart::{HartId, HartWake},
    memory::MemoryRegistry,
};
use spin::Once;

use crate::{
    driver::{Description as _, device::cci},
    firmware::warm_entry,
};

pub(crate) use description::Desc;
use reset_vector::ResetVectorRegisters;

pub(crate) fn describe(platform: &PlatformView<'_>) -> Result<Option<Desc>> {
    Ok(platform.spacemit_k1_registers()?.map(Desc::new))
}

/// Initializes the K1 platform and returns its hart wake device.
///
/// The required register windows are acquired before hardware state changes.
pub(crate) fn init(description: Desc, memory: &mut MemoryRegistry) -> Result<Box<dyn HartWake>> {
    let registers = description.registers;
    let wakeup = Box::new(wakeup::Desc::new(registers)).init(memory)?;
    let reset_vectors = ResetVectorRegisters::acquire(memory, registers.reset_vectors())?;
    let cci = cci::Cci550::init(
        cci::Desc {
            status: registers.cci_status(),
            snoop_controls: registers.cci_snoop_controls(),
        },
        memory,
    )?;

    HART_REGISTERS.call_once(|| registers);
    initialize_hart(registers);
    reset_vectors.set_reset_vector(warm_entry as *const () as usize as u64)?;
    cci.enable_coherency()?;
    Ok(Box::new(wakeup))
}

/// Performs the K1 per-hart L2 and machine-feature setup.
fn initialize_hart(registers: SpacemitK1Registers) {
    registers.enable_hart_l2(
        HartId::current()
            .expect("BUG: current hart exceeds Runtime capacity")
            .as_usize(),
    );
    registers.enable_machine_features();
}

static HART_REGISTERS: Once<SpacemitK1Registers> = Once::new();

/// Runs K1's per-hart setup when this platform provides its registers.
pub(crate) fn initialize_secondary_hart() {
    if let Some(registers) = HART_REGISTERS.get() {
        initialize_hart(*registers);
    }
}
