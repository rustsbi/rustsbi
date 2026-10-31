//! QEMU AIA machines' M-level APLIC setup.
//!
//! Specification: [RISC-V AIA 1.0], sections 4.5.2–4.5.4 and 4.5.11,
//! defines the register semantics. Platform sources: the pinned [QEMU `virt`]
//! and [`riscv-server-ref`] machines share this M-domain layout; addresses and
//! the source count come from each machine's device tree, and the pinned
//! [APLIC header] defines the register-window size.
//!
//! [RISC-V AIA 1.0]: https://docs.riscv.org/reference/aia/_attachments/riscv-interrupts.pdf
//! [QEMU `virt`]: https://gitlab.com/qemu-project/qemu/-/blob/99e54ab5e7a6efc945af6d5661842155d1f3fc7a/hw/riscv/virt.c
//! [`riscv-server-ref`]: https://github.com/qemu/qemu/blob/d73149283205645625bfa9fe029cfb68e4c4a9ae/hw/riscv/server_platform_ref.c
//! [APLIC header]: https://gitlab.com/qemu-project/qemu/-/blob/99e54ab5e7a6efc945af6d5661842155d1f3fc7a/include/hw/intc/riscv_aplic.h

mod registers;

use runtime::{
    FdtNode, Result,
    memory::{DeviceRegisterRange, MemoryRegistry, PhysAddr},
};

use registers::{AplicRegisters, EncodedMsiAddress};

use super::info::MachineAplicDescription;

const APLIC_COMPATIBLE: &str = "riscv,aplic";

/// A QEMU machine's M-APLIC resources awaiting Runtime binding.
pub(crate) struct QemuAplicConfig {
    registers: DeviceRegisterRange,
    machine_msi: EncodedMsiAddress,
    supervisor_msi: EncodedMsiAddress,
    num_sources: usize,
}

impl QemuAplicConfig {
    /// Builds the M-APLIC setup discovered in the Platform Description.
    pub(crate) fn new(
        description: &MachineAplicDescription,
        machine_imsic_base: PhysAddr,
        hart_index_bits: u32,
    ) -> Result<Self> {
        Ok(Self {
            registers: description.registers,
            machine_msi: EncodedMsiAddress::machine(machine_imsic_base, hart_index_bits)?,
            supervisor_msi: EncodedMsiAddress::supervisor(
                description.supervisor_imsic_base,
                hart_index_bits,
            )?,
            num_sources: description.num_sources,
        })
    }

    /// Acquires and configures the machine APLIC register block.
    pub(crate) fn bind(self, memory: &mut MemoryRegistry) -> Result<()> {
        let registers = memory.acquire(self.registers)?;
        let msi_configuration_locked = AplicRegisters::new(registers)
            .configure_and_delegate_sources(
                self.machine_msi,
                self.supervisor_msi,
                self.num_sources,
            )?;
        if msi_configuration_locked {
            warn!("AIA: M-level APLIC MSI configuration is locked");
        }
        info!(
            "AIA: delegated M-level APLIC IRQs 1..={} to S-level child",
            self.num_sources
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
