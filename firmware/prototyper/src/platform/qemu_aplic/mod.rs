//! QEMU `virt` routing from the machine APLIC domain to supervisor interrupts.
//!
//! Specification: [RISC-V AIA 1.0], sections 4.5.2–4.5.4 and 4.5.11,
//! defines the register semantics. Platform source: pinned [QEMU `virt`]
//! defines addresses and source count; its pinned [APLIC header] defines the
//! register-window size.
//!
//! [RISC-V AIA 1.0]: https://docs.riscv.org/reference/aia/_attachments/riscv-interrupts.pdf
//! [QEMU `virt`]: https://gitlab.com/qemu-project/qemu/-/blob/99e54ab5e7a6efc945af6d5661842155d1f3fc7a/hw/riscv/virt.c
//! [APLIC header]: https://gitlab.com/qemu-project/qemu/-/blob/99e54ab5e7a6efc945af6d5661842155d1f3fc7a/include/hw/intc/riscv_aplic.h

use runtime::FdtNode;
use runtime::memory::{DeviceRegisterRange, MemoryRegistry, PhysAddr};

use crate::driver::aplic::{Aplic, MsiAddress};

const SUPERVISOR_IMSIC_BASE: PhysAddr = PhysAddr::new(0x2800_0000);
const INTERRUPT_SOURCE_COUNT: usize = 96;
const APLIC_COMPATIBLE: &str = "riscv,aplic";

/// QEMU `virt` M-APLIC register range and encoded MSI destinations for binding.
pub(crate) struct QemuAplicConfig {
    registers: DeviceRegisterRange,
    machine_msi: MsiAddress,
    supervisor_msi: MsiAddress,
}

impl QemuAplicConfig {
    /// Builds the QEMU `virt` M-APLIC setup discovered in the Platform Description.
    pub(crate) fn new(
        registers: DeviceRegisterRange,
        machine_imsic_base: PhysAddr,
        hart_index_bits: u32,
    ) -> runtime::Result<Self> {
        Ok(Self {
            registers,
            machine_msi: MsiAddress::new(machine_imsic_base, hart_index_bits)?,
            // QEMU 10.1 reads the supervisor domain's hart-index width from
            // smsiaddrcfgh, so repeat the machine layout field there.
            supervisor_msi: MsiAddress::new(SUPERVISOR_IMSIC_BASE, hart_index_bits)?,
        })
    }

    /// Acquires and configures the QEMU M-APLIC register block.
    pub(crate) fn bind(self, memory: &mut MemoryRegistry) -> runtime::Result<()> {
        let registers = memory.acquire_mmio(self.registers)?;
        let msi_configuration_locked = Aplic::new(registers).configure_and_delegate_sources(
            self.machine_msi,
            self.supervisor_msi,
            INTERRUPT_SOURCE_COUNT,
        )?;
        if msi_configuration_locked {
            warn!("AIA: M-level APLIC MSI configuration is locked");
        }
        info!(
            "AIA: delegated M-level APLIC IRQs 1..={} to S-level child",
            INTERRUPT_SOURCE_COUNT
        );
        Ok(())
    }
}

/// Returns whether an APLIC node describes a machine-level domain.
pub(crate) fn is_machine_domain(node: FdtNode<'_, '_>, compatible: &str) -> bool {
    compatible == APLIC_COMPATIBLE && node.property("riscv,children").is_some()
}

/// Returns whether the machine-level domain is delegated to the next-stage
/// supervisor and therefore must be hidden when firmware keeps IMSIC IPIs.
pub(crate) fn is_delegated_machine_domain(node: FdtNode<'_, '_>, compatible: &str) -> bool {
    is_machine_domain(node, compatible)
        && (node.property("riscv,delegate").is_some()
            || node.property("riscv,delegation").is_some())
}
