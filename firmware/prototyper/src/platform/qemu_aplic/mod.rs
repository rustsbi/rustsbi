//! QEMU `virt` M-level APLIC setup.
//!
//! Specification: [RISC-V AIA 1.0], sections 4.5.2–4.5.4 and 4.5.11,
//! defines the register semantics. Platform source: pinned [QEMU `virt`]
//! defines addresses and source count; its pinned [APLIC header] defines the
//! register-window size.
//!
//! [RISC-V AIA 1.0]: https://docs.riscv.org/reference/aia/_attachments/riscv-interrupts.pdf
//! [QEMU `virt`]: https://gitlab.com/qemu-project/qemu/-/blob/99e54ab5e7a6efc945af6d5661842155d1f3fc7a/hw/riscv/virt.c
//! [APLIC header]: https://gitlab.com/qemu-project/qemu/-/blob/99e54ab5e7a6efc945af6d5661842155d1f3fc7a/include/hw/intc/riscv_aplic.h

mod registers;

use runtime::memory::{DeviceRegisterRange, MemoryRegistry, PhysAddr};
use serde_device_tree::buildin::Node;

use registers::{AplicRegisters, EncodedMsiAddress};

const SUPERVISOR_IMSIC_BASE: PhysAddr = PhysAddr::new(0x2800_0000);
const INTERRUPT_SOURCE_COUNT: usize = 96;
const APLIC_COMPATIBLE: &str = "riscv,aplic";

/// QEMU `virt` M-APLIC resources awaiting Runtime binding.
pub(crate) struct QemuAplicConfig {
    registers: DeviceRegisterRange,
    machine_msi: EncodedMsiAddress,
    supervisor_msi: EncodedMsiAddress,
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
            machine_msi: EncodedMsiAddress::machine(machine_imsic_base, hart_index_bits)?,
            supervisor_msi: EncodedMsiAddress::supervisor(SUPERVISOR_IMSIC_BASE, hart_index_bits)?,
        })
    }

    /// Acquires and configures the QEMU M-APLIC register block.
    pub(crate) fn bind(self, memory: &mut MemoryRegistry) -> runtime::Result<()> {
        let registers = memory.acquire_mmio(self.registers)?;
        let msi_configuration_locked = AplicRegisters::new(registers)
            .configure_and_delegate_sources(
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
pub(crate) fn is_machine_domain(node: &Node<'_>, compatible: &str) -> bool {
    compatible == APLIC_COMPATIBLE && node.get_prop("riscv,children").is_some()
}
