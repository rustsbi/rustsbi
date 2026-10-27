//! APLIC configuration and register programming.
//!
//! Specification: [RISC-V AIA 1.0], sections 4.5.2–4.5.4 and 4.5.11,
//! defines the register semantics. Board-specific MSI targets and source
//! counts are supplied by board quirks such as [`qemu`].
//!
//! [RISC-V AIA 1.0]: https://docs.riscv.org/reference/aia/_attachments/riscv-interrupts.pdf
//! [QEMU `virt`]: https://gitlab.com/qemu-project/qemu/-/blob/99e54ab5e7a6efc945af6d5661842155d1f3fc7a/hw/riscv/virt.c
//! [APLIC header]: https://gitlab.com/qemu-project/qemu/-/blob/99e54ab5e7a6efc945af6d5661842155d1f3fc7a/include/hw/intc/riscv_aplic.h

mod qemu;
mod registers;

pub(crate) use qemu::QEMU_VIRT_MODEL;

use runtime::{
    FdtNode, PlatformView, Result,
    memory::{DeviceRegisterRange, MemoryRegistry, PhysAddr},
};

use crate::driver::{DeviceFacts, ImsicInfo};

use registers::{AplicRegisters, EncodedMsiAddress};

const APLIC_COMPATIBLE: &str = "riscv,aplic";

/// QEMU `virt` M-APLIC resources awaiting Runtime binding.
pub(crate) struct AplicConfig {
    registers: DeviceRegisterRange,
    machine_msi: EncodedMsiAddress,
    supervisor_msi: EncodedMsiAddress,
    source_count: usize,
}

impl AplicConfig {
    /// Builds an APLIC configuration from the discovered range and MSI targets.
    pub(crate) fn new(
        registers: DeviceRegisterRange,
        machine_imsic_base: PhysAddr,
        supervisor_imsic_base: PhysAddr,
        hart_index_bits: u32,
        source_count: usize,
    ) -> Result<Self> {
        Ok(Self {
            registers,
            machine_msi: EncodedMsiAddress::machine(machine_imsic_base, hart_index_bits)?,
            supervisor_msi: EncodedMsiAddress::supervisor(supervisor_imsic_base, hart_index_bits)?,
            source_count,
        })
    }

    /// Acquires and configures the QEMU M-APLIC register block.
    pub(crate) fn bind(self, memory: &mut MemoryRegistry) -> Result<()> {
        let registers = memory.acquire(self.registers)?;
        let msi_configuration_locked = AplicRegisters::new(registers)
            .configure_and_delegate_sources(
                self.machine_msi,
                self.supervisor_msi,
                self.source_count,
            )?;
        if msi_configuration_locked {
            warn!("AIA: M-level APLIC MSI configuration is locked");
        }
        info!(
            "AIA: delegated M-level APLIC IRQs 1..={} to S-level child",
            self.source_count
        );
        Ok(())
    }
}

/// Selects the board-specific APLIC configuration, if any.
pub(crate) fn config(
    platform: &PlatformView<'_>,
    facts: &DeviceFacts<'_>,
    imsic: &ImsicInfo,
) -> Result<Option<AplicConfig>> {
    qemu::config(platform, facts, imsic)
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
