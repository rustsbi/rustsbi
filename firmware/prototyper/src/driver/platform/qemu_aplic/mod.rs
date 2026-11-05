//! QEMU `virt` M-level APLIC setup.
//!
//! [RISC-V AIA 1.0], sections 4.5.2–4.5.4 and 4.5.11, defines the register semantics.
//! The pinned [QEMU `virt`] source defines addresses and source count;
//! its pinned [APLIC header] defines the register-window size.
//!
//! [RISC-V AIA 1.0]: https://docs.riscv.org/reference/aia/_attachments/riscv-interrupts.pdf
//! [QEMU `virt`]: https://gitlab.com/qemu-project/qemu/-/blob/99e54ab5e7a6efc945af6d5661842155d1f3fc7a/hw/riscv/virt.c
//! [APLIC header]: https://gitlab.com/qemu-project/qemu/-/blob/99e54ab5e7a6efc945af6d5661842155d1f3fc7a/include/hw/intc/riscv_aplic.h

mod registers;

use runtime::{
    Error, Result,
    memory::{MemoryRegistry, PhysAddr, RegRange},
};

use crate::firmware::IrqMmio;
use registers::{AplicRegisters, EncodedMsiAddress};

const SUPERVISOR_IMSIC_BASE: PhysAddr = PhysAddr::new(0x2800_0000);
const INTERRUPT_SOURCE_COUNT: usize = 96;

/// Acquires and configures QEMU `virt`'s M-APLIC register block.
pub(crate) fn init(
    registers: RegRange,
    machine_imsic_base: PhysAddr,
    hart_index_bits: u32,
    memory: &mut MemoryRegistry,
) -> Result<()> {
    let machine_msi = EncodedMsiAddress::machine(machine_imsic_base, hart_index_bits)?;
    let supervisor_msi = EncodedMsiAddress::supervisor(SUPERVISOR_IMSIC_BASE, hart_index_bits)?;
    let registers = memory.acquire(registers)?;
    let msi_configuration_locked = AplicRegisters::new(registers)
        .configure_and_delegate_sources(machine_msi, supervisor_msi)?;
    if msi_configuration_locked {
        warn!("AIA: M-level APLIC MSI configuration is locked");
    }
    Ok(())
}

/// Configures QEMU's interrupt routing and records the MMIO ranges for PMP.
pub(super) fn init_for_imsic(
    model: &str,
    aplic: Option<RegRange>,
    clint: Option<RegRange>,
    imsic_base: PhysAddr,
    hart_index_bits: u32,
    imsic_end: Option<usize>,
    memory: &mut MemoryRegistry,
) -> Result<Option<IrqMmio>> {
    if model != "riscv-virtio,qemu" {
        return Ok(None);
    }
    let aplic = aplic.ok_or(Error::InvalidArgs)?;
    init(aplic, imsic_base, hart_index_bits, memory)?;
    Ok(Some(IrqMmio {
        clint_start: clint.map_or(0x0200_0000, |range| range.start().as_usize()),
        aplic,
        imsic_start: imsic_base.as_usize(),
        imsic_end: imsic_end.ok_or(Error::InvalidArgs)?,
    }))
}
