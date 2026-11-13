//! QEMU AIA machines' M-level APLIC setup.
//!
//! [RISC-V AIA 1.0], sections 4.5.2–4.5.4 and 4.5.11, defines the register
//! semantics. Both QEMU AIA machines (`virt` with `aia=aplic-imsic` and
//! `riscv-server-ref`) describe their machine-level APLIC wiring through the
//! device tree, which [`describe`] parses; the pinned [QEMU `virt`] and
//! [server platform reference] machines define the described layouts, and the
//! pinned [APLIC header] defines the register-window size.
//!
//! [RISC-V AIA 1.0]: https://docs.riscv.org/reference/aia/_attachments/riscv-interrupts.pdf
//! [QEMU `virt`]: https://gitlab.com/qemu-project/qemu/-/blob/99e54ab5e7a6efc945af6d5661842155d1f3fc7a/hw/riscv/virt.c
//! [server platform reference]: https://github.com/qemu/qemu/blob/d73149283205645625bfa9fe029cfb68e4c4a9ae/hw/riscv/server_platform_ref.c
//! [APLIC header]: https://gitlab.com/qemu-project/qemu/-/blob/99e54ab5e7a6efc945af6d5661842155d1f3fc7a/include/hw/intc/riscv_aplic.h

mod registers;

use runtime::{
    Error, FdtNode, PlatformView, Result,
    memory::{MemoryRegistry, PhysAddr, RegRange},
};

use crate::devicetree::{find_phandle, u32_property};
use crate::firmware::IrqMmio;
use registers::{AplicRegisters, EncodedMsiAddress};

/// A machine-level APLIC described by the device tree.
pub(crate) struct Desc {
    pub(crate) registers: RegRange,
    /// Source count from the node's `riscv,num-sources`.
    num_sources: usize,
    /// Base of the supervisor-level IMSIC group receiving delegated MSIs,
    /// resolved through the child APLIC's `msi-parent`.
    supervisor_imsic_base: PhysAddr,
}

/// Parses one machine-domain APLIC node's delegation wiring.
///
/// The source count comes from the node's own `riscv,num-sources`, and the
/// supervisor MSI target is resolved through the child APLIC named by
/// `riscv,children`: its `msi-parent` names the supervisor-level IMSIC group
/// whose first register range supplies the group base.
pub(crate) fn describe(platform: &PlatformView<'_>, node: FdtNode<'_, '_>) -> Result<Desc> {
    let registers = platform.device_register(node)?.ok_or(Error::InvalidArgs)?;
    let num_sources = u32_property(node, "riscv,num-sources")
        .filter(|sources| *sources > 0)
        .and_then(|sources| usize::try_from(sources).ok())
        .ok_or(Error::InvalidArgs)?;

    let child_phandle = u32_property(node, "riscv,children").ok_or(Error::InvalidArgs)?;
    let child = find_phandle(platform.root(), child_phandle).ok_or(Error::InvalidArgs)?;
    let msi_phandle = u32_property(child, "msi-parent").ok_or(Error::InvalidArgs)?;
    let supervisor_imsic = find_phandle(platform.root(), msi_phandle).ok_or(Error::InvalidArgs)?;
    let supervisor_imsic_base = platform
        .device_registers(supervisor_imsic)?
        .and_then(|registers| registers.first().copied())
        .map(|range| range.start())
        .ok_or(Error::InvalidArgs)?;

    Ok(Desc {
        registers,
        num_sources,
        supervisor_imsic_base,
    })
}

/// Acquires and configures the machine APLIC register block.
fn init(
    desc: &Desc,
    machine_imsic_base: PhysAddr,
    hart_index_bits: u32,
    memory: &mut MemoryRegistry,
) -> Result<()> {
    let machine_msi = EncodedMsiAddress::machine(machine_imsic_base, hart_index_bits)?;
    let supervisor_msi =
        EncodedMsiAddress::supervisor(desc.supervisor_imsic_base, hart_index_bits)?;
    let registers = memory.acquire(desc.registers)?;
    let msi_configuration_locked = AplicRegisters::new(registers).configure_and_delegate_sources(
        machine_msi,
        supervisor_msi,
        desc.num_sources,
    )?;
    if msi_configuration_locked {
        warn!("AIA: M-level APLIC MSI configuration is locked");
    }
    Ok(())
}

/// Delegates a described machine APLIC to its supervisor child and records
/// the MMIO ranges for PMP.
pub(super) fn init_for_imsic(
    desc: Desc,
    clint: Option<RegRange>,
    imsic_base: PhysAddr,
    hart_index_bits: u32,
    imsic_end: Option<usize>,
    memory: &mut MemoryRegistry,
) -> Result<IrqMmio> {
    init(&desc, imsic_base, hart_index_bits, memory)?;
    let imsic_end = imsic_end.ok_or(Error::InvalidArgs)?;
    Ok(IrqMmio {
        // Both QEMU AIA machines place their ACLINT at 0x0200_0000.
        clint_start: clint.map_or(0x0200_0000, |range| range.start().as_usize()),
        aplic: desc.registers,
        imsic_start: imsic_base.as_usize(),
        imsic_end,
    })
}
