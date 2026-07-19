//! Retained QEMU machine-APLIC delegation protocol.

use core::ops::Range;

use dtoolkit::Node;
use dtoolkit::fdt::Fdt;

use crate::boot::BootInfo;
use crate::boot::device_tree::{
    BindingError, compatible, enabled, exact_node, model, reg_ranges, u32_property,
};

mod arch;

const QEMU_MODEL: &str = "riscv-virtio,qemu";
const QEMU_MACHINE_BASE: usize = 0x0c00_0000;
#[cfg(test)]
const QEMU_MACHINE_IMSIC_BASE: u64 = 0x2400_0000;
const QEMU_SUPERVISOR_IMSIC_BASE: u64 = 0x2800_0000;
const QEMU_SOURCE_COUNT: u32 = 96;
const MINIMUM_SIZE: usize = 0x4000;
const DOMAINCFG: usize = 0x0000;
const SOURCECFG_BASE: usize = 0x0004;
const MMSICFGADDR: usize = 0x1bc0;
const MMSICFGADDRH: usize = 0x1bc4;
const SMSICFGADDR: usize = 0x1bc8;
const SMSICFGADDRH: usize = 0x1bcc;
const CLRIE_BASE: usize = 0x1f00;
const DOMAINCFG_IE: u32 = 1 << 8;
const DOMAINCFG_DM: u32 = 1 << 2;
const DOMAINCFG_BE: u32 = 1;
const DOMAINCFG_POLICY_MASK: u32 = DOMAINCFG_IE | DOMAINCFG_DM | DOMAINCFG_BE;
const SOURCECFG_DELEGATE: u32 = 1 << 10;
const MSICFGADDRH_LOCK: u32 = 1 << 31;
const MSICFGADDRH_LHXW_SHIFT: u32 = 12;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum AplicError {
    Binding(BindingError),
    Unauthorized,
    InvalidConfiguration,
    Locked,
    Readback,
}

pub(super) struct Binding {
    pub(super) range: Range<usize>,
    source_count: u32,
}

impl Binding {
    pub(super) fn from_dtb(boot: &BootInfo, path: &str) -> Result<Self, AplicError> {
        let fdt = Fdt::new(boot.dtb().as_bytes())
            .map_err(|_| AplicError::Binding(BindingError::DeviceTree))?;
        let node = exact_node(&fdt, path).map_err(AplicError::Binding)?;
        if !enabled(&node)
            || !compatible(&node, &["riscv,aplic"])
            || node.property("riscv,children").is_none()
        {
            return Err(AplicError::Binding(BindingError::Unsupported));
        }
        let ranges = reg_ranges(node).map_err(AplicError::Binding)?;
        if ranges.len() != 1 {
            return Err(AplicError::InvalidConfiguration);
        }
        let range = ranges.into_iter().next().unwrap();
        let source_count = u32_property(&node, "riscv,num-sources").map_err(AplicError::Binding)?;
        let canonical = model(&fdt) == QEMU_MODEL
            && range.start == QEMU_MACHINE_BASE
            && range.end >= QEMU_MACHINE_BASE + MINIMUM_SIZE
            && source_count == QEMU_SOURCE_COUNT;
        if !canonical {
            return Err(AplicError::Unauthorized);
        }
        Ok(Self {
            range,
            source_count,
        })
    }

    pub(super) fn configure(
        &self,
        machine_imsic_base: usize,
        hart_index_bits: u32,
    ) -> Result<(), AplicError> {
        if hart_index_bits > 7 {
            return Err(AplicError::InvalidConfiguration);
        }
        arch::configure_device(
            self.range.start,
            self.source_count,
            u64::try_from(machine_imsic_base).map_err(|_| AplicError::InvalidConfiguration)?,
            QEMU_SUPERVISOR_IMSIC_BASE,
            hart_index_bits,
        )
    }
}

pub(in crate::drivers::aplic) trait Registers {
    fn read(&mut self, offset: usize) -> u32;
    fn write(&mut self, offset: usize, value: u32);
    fn fence(&mut self);
}

pub(in crate::drivers::aplic) fn configure<R: Registers>(
    registers: &mut R,
    source_count: u32,
    machine_imsic_base: u64,
    supervisor_imsic_base: u64,
    hart_index_bits: u32,
) -> Result<(), AplicError> {
    if source_count == 0 || hart_index_bits > 7 {
        return Err(AplicError::InvalidConfiguration);
    }
    if registers.read(MMSICFGADDRH) & MSICFGADDRH_LOCK != 0 {
        return Err(AplicError::Locked);
    }

    // Close delivery before changing routing. `domaincfg` contains a
    // read-only identification byte and WARL mode bits, so whole-register
    // equality is not a valid readback rule. The retained driver requires MSI
    // delivery mode, little-endian register accesses, and disabled delivery;
    // every other visible bit is either reserved or hardware-owned.
    registers.write(DOMAINCFG, DOMAINCFG_DM);
    if registers.read(DOMAINCFG) & DOMAINCFG_POLICY_MASK != DOMAINCFG_DM {
        return Err(AplicError::Readback);
    }
    for first_source in (0..=source_count).step_by(32) {
        let word =
            usize::try_from(first_source / 32).map_err(|_| AplicError::InvalidConfiguration)?;
        registers.write(CLRIE_BASE + word * 4, u32::MAX);
    }
    let (machine_low, machine_high) = msi_config(machine_imsic_base, hart_index_bits)?;
    let (supervisor_low, supervisor_high) = msi_config(supervisor_imsic_base, hart_index_bits)?;
    write_checked(registers, MMSICFGADDR, machine_low)?;
    write_checked(registers, MMSICFGADDRH, machine_high)?;
    write_checked(registers, SMSICFGADDR, supervisor_low)?;
    write_checked(registers, SMSICFGADDRH, supervisor_high)?;
    for source in 1..=source_count {
        let offset = SOURCECFG_BASE
            .checked_add(
                usize::try_from(source - 1)
                    .map_err(|_| AplicError::InvalidConfiguration)?
                    .checked_mul(4)
                    .ok_or(AplicError::InvalidConfiguration)?,
            )
            .ok_or(AplicError::InvalidConfiguration)?;
        write_checked(registers, offset, SOURCECFG_DELEGATE)?;
    }
    registers.fence();
    Ok(())
}

fn write_checked<R: Registers>(
    registers: &mut R,
    offset: usize,
    value: u32,
) -> Result<(), AplicError> {
    registers.write(offset, value);
    if registers.read(offset) == value {
        Ok(())
    } else {
        Err(AplicError::Readback)
    }
}

fn msi_config(base: u64, hart_index_bits: u32) -> Result<(u32, u32), AplicError> {
    if !base.is_multiple_of(0x1000) || hart_index_bits > 7 {
        return Err(AplicError::InvalidConfiguration);
    }
    let mut base_ppn = base >> 12;
    let hart_mask = 1u64
        .checked_shl(hart_index_bits)
        .ok_or(AplicError::InvalidConfiguration)?
        .wrapping_sub(1);
    base_ppn &= !hart_mask;
    Ok((
        base_ppn as u32,
        ((base_ppn >> 32) as u32) | (hart_index_bits << MSICFGADDRH_LHXW_SHIFT),
    ))
}

#[cfg(test)]
mod tests {
    use alloc::collections::BTreeMap;
    use alloc::vec::Vec;

    use super::*;

    #[derive(Default)]
    struct FakeRegisters {
        values: BTreeMap<usize, u32>,
        read_only: BTreeMap<usize, u32>,
        writes: Vec<(usize, u32)>,
        fences: usize,
        corrupt_readback: Option<usize>,
    }

    impl Registers for FakeRegisters {
        fn read(&mut self, offset: usize) -> u32 {
            let value = self.values.get(&offset).copied().unwrap_or(0)
                | self.read_only.get(&offset).copied().unwrap_or(0);
            if self.corrupt_readback == Some(offset) {
                value ^ 1
            } else {
                value
            }
        }

        fn write(&mut self, offset: usize, value: u32) {
            self.values.insert(offset, value);
            self.writes.push((offset, value));
        }

        fn fence(&mut self) {
            self.fences += 1;
        }
    }

    #[test]
    fn qemu_delegation_is_read_back_and_fenced() {
        let mut registers = FakeRegisters::default();
        registers.read_only.insert(DOMAINCFG, 1 << 31);
        assert_eq!(
            configure(
                &mut registers,
                QEMU_SOURCE_COUNT,
                QEMU_MACHINE_IMSIC_BASE,
                QEMU_SUPERVISOR_IMSIC_BASE,
                2,
            ),
            Ok(())
        );
        assert_eq!(registers.fences, 1);
        assert_eq!(registers.values[&DOMAINCFG], DOMAINCFG_DM);
        assert_eq!(
            registers.values[&(SOURCECFG_BASE + (QEMU_SOURCE_COUNT as usize - 1) * 4)],
            SOURCECFG_DELEGATE
        );
        assert_eq!(registers.values[&MMSICFGADDR], 0x24000);
        assert_eq!(registers.values[&SMSICFGADDR], 0x28000);
    }

    #[test]
    fn locked_or_failed_readback_never_reaches_the_final_fence() {
        let mut locked = FakeRegisters::default();
        locked.values.insert(MMSICFGADDRH, MSICFGADDRH_LOCK);
        assert_eq!(
            configure(&mut locked, 1, QEMU_MACHINE_BASE as u64, 0x2800_0000, 0),
            Err(AplicError::Locked)
        );
        assert!(locked.writes.is_empty());

        let mut corrupt = FakeRegisters {
            corrupt_readback: Some(MMSICFGADDR),
            ..FakeRegisters::default()
        };
        assert_eq!(
            configure(&mut corrupt, 1, QEMU_MACHINE_BASE as u64, 0x2800_0000, 0),
            Err(AplicError::Readback)
        );
        assert_eq!(corrupt.fences, 0);
    }

    #[test]
    fn msi_encoding_uses_the_full_physical_address_width() {
        assert_eq!(msi_config(0x1_2400_0000, 2), Ok((0x12_4000, 2 << 12)));
    }
}
