//! APLIC routing owned by one validated MMIO capability.

use crate::{IoMem, io_fence};

const DOMAINCFG: usize = 0x0000;
const SOURCECFG_BASE: usize = 0x0004;
const MMSICFGADDR: usize = 0x1bc0;
const MMSICFGADDRH: usize = 0x1bc4;
const SMSICFGADDR: usize = 0x1bc8;
const SMSICFGADDRH: usize = 0x1bcc;
const CLRIE_BASE: usize = 0x1f00;

bitflags::bitflags! {
    /// Writable APLIC domain policy bits required by firmware routing.
    #[derive(Clone, Copy, Eq, PartialEq)]
    struct DomainConfig: u32 {
        const BIG_ENDIAN = 1 << 0;
        const MSI_DELIVERY = 1 << 2;
        const DELIVERY_ENABLE = 1 << 8;
    }
}

bitflags::bitflags! {
    /// Machine MSI configuration-high bits used by the retained path.
    #[derive(Clone, Copy, Eq, PartialEq)]
    struct MsiConfigHigh: u32 {
        const LOCKED = 1 << 31;
    }
}

const SOURCE_DELEGATE: u32 = 1 << 10;
const HART_INDEX_SHIFT: u32 = 12;

pub(super) fn configure(
    registers: &IoMem,
    source_count: u32,
    machine_imsic_base: u64,
    supervisor_imsic_base: u64,
    hart_index_width: u32,
) -> Option<()> {
    if source_count == 0 || hart_index_width > 7 {
        return None;
    }
    let high = registers.read_once::<u32>(MMSICFGADDRH).ok()?;
    if MsiConfigHigh::from_bits_retain(high).contains(MsiConfigHigh::LOCKED) {
        return None;
    }

    let domain = DomainConfig::MSI_DELIVERY;
    write(registers, DOMAINCFG, domain.bits())?;
    let visible = DomainConfig::from_bits_retain(registers.read_once::<u32>(DOMAINCFG).ok()?);
    if visible.intersects(
        DomainConfig::BIG_ENDIAN | DomainConfig::MSI_DELIVERY | DomainConfig::DELIVERY_ENABLE,
    ) && visible != domain
    {
        return None;
    }

    for source in (0..=source_count).step_by(32) {
        let word = usize::try_from(source / 32).ok()?;
        registers
            .write_once(CLRIE_BASE.checked_add(word.checked_mul(4)?)?, u32::MAX)
            .ok()?;
    }
    let (machine_low, machine_high) = msi_config(machine_imsic_base, hart_index_width)?;
    let (supervisor_low, supervisor_high) = msi_config(supervisor_imsic_base, hart_index_width)?;
    write(registers, MMSICFGADDR, machine_low)?;
    write(registers, MMSICFGADDRH, machine_high)?;
    write(registers, SMSICFGADDR, supervisor_low)?;
    write(registers, SMSICFGADDRH, supervisor_high)?;
    for source in 1..=source_count {
        let offset =
            SOURCECFG_BASE.checked_add(usize::try_from(source - 1).ok()?.checked_mul(4)?)?;
        write(registers, offset, SOURCE_DELEGATE)?;
    }
    io_fence();
    Some(())
}

fn write(registers: &IoMem, offset: usize, value: u32) -> Option<()> {
    registers.write_once(offset, value).ok()?;
    (registers.read_once::<u32>(offset).ok()? == value).then_some(())
}

fn msi_config(base: u64, hart_index_width: u32) -> Option<(u32, u32)> {
    if !base.is_multiple_of(0x1000) || hart_index_width > 7 {
        return None;
    }
    let hart_mask = 1u64.checked_shl(hart_index_width)?.wrapping_sub(1);
    let page_number = (base >> 12) & !hart_mask;
    Some((
        page_number as u32,
        ((page_number >> 32) as u32) | (hart_index_width << HART_INDEX_SHIFT),
    ))
}
