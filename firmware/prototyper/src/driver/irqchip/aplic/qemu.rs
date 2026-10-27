//! QEMU `virt` APLIC quirk.
//!
//! QEMU is handled by the generic FDT platform; only its machine-level APLIC
//! wiring differs. This driver quirk supplies that wiring when the root
//! model identifies QEMU `virt`.

use runtime::{Error, PlatformView, Result, memory::PhysAddr};

use crate::driver::{DeviceFacts, ImsicInfo};

use super::AplicConfig;

const SUPERVISOR_IMSIC_BASE: PhysAddr = PhysAddr::new(0x2800_0000);
const INTERRUPT_SOURCE_COUNT: usize = 96;
/// Root model identifying the QEMU `virt` interrupt-controller layout.
pub(crate) const QEMU_VIRT_MODEL: &str = "riscv-virtio,qemu";

/// Builds the QEMU APLIC configuration when QEMU retains the machine-level
/// interrupt files for firmware use.
pub(super) fn config(
    platform: &PlatformView<'_>,
    facts: &DeviceFacts<'_>,
    imsic: &ImsicInfo,
) -> Result<Option<AplicConfig>> {
    if platform
        .root()
        .property("model")
        .and_then(|property| property.as_str())
        != Some(QEMU_VIRT_MODEL)
    {
        return Ok(None);
    }
    let registers = facts.machine_aplic.ok_or(Error::InvalidArgs)?;
    Ok(Some(AplicConfig::new(
        registers,
        imsic.layout.machine_base,
        SUPERVISOR_IMSIC_BASE,
        imsic.layout.hart_index_bits,
        INTERRUPT_SOURCE_COUNT,
    )?))
}
