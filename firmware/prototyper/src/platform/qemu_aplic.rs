//! QEMU `virt` M-level APLIC delegation.

use crate::platform::{BoardInfo, mmio::Mmio};

/// Fixed QEMU `virt` address of the M-level APLIC.
pub(crate) const QEMU_VIRT_M_APLIC_BASE: usize = 0x0c00_0000;
const QEMU_VIRT_S_IMSIC_BASE: usize = 0x2800_0000;
const QEMU_VIRT_APLIC_NUM_SOURCES: usize = 96;
/// MMIO region reserved by QEMU for the M-level APLIC.
pub(crate) const APLIC_SPAN: usize = 0x8000;

// Register offsets (AIA v1.0 §4.5, Table 4.1).
const DOMAINCFG: usize = 0x0000;
const SOURCECFG: usize = 0x0004;
const MMSICFGADDR: usize = 0x1bc0;
const MMSICFGADDRH: usize = 0x1bc4;
const SMSICFGADDR: usize = 0x1bc8;
const SMSICFGADDRH: usize = 0x1bcc;
const CLRIE: usize = 0x1f00;

/// `sourcecfg.D` (delegation) is bit 10; bits 9:0 select the child domain.
const SOURCECFG_DELEGATE: u32 = 1 << 10;
/// `*msicfgaddrh.LOCK` is bit 31; once set, the MSI configuration is immutable.
const MSICFGADDRH_LOCK: u32 = 1 << 31;
/// `*msicfgaddrh.LHXW` (low hart-index width) occupies bits 15:12.
const MSICFGADDRH_LHXW_SHIFT: u32 = 12;

/// Configures QEMU's M-level APLIC and delegates its sources to the S-level child.
///
/// A locked MSI configuration is left intact while source delegation is
/// refreshed, matching the pre-MMIO driver behavior.
pub(crate) fn init_qemu_m_aplic_delegation(
    board: &BoardInfo,
    machine_imsic_base: usize,
    hart_index_bits: u32,
) -> bool {
    let Some(aplic) = Mmio::within(board, QEMU_VIRT_M_APLIC_BASE, APLIC_SPAN) else {
        warn!("AIA: QEMU M-level APLIC MMIO window is unavailable");
        return false;
    };
    let msi_config_locked = aplic.read::<u32>(MMSICFGADDRH) & MSICFGADDRH_LOCK != 0;
    if msi_config_locked {
        warn!("AIA: M-level APLIC MSI configuration is locked");
    } else {
        // Per AIA v1.0 §4.5, each m/smsicfgaddr pair targets one IMSIC group:
        // the group's base PPN with the hart-index bits cleared, while the
        // matching *msicfgaddrh.LHXW field tells the APLIC how many low PPN
        // bits to replace with the destination hart index when it computes
        // each MSI target address.
        for &(address, address_high, imsic_base) in &[
            (MMSICFGADDR, MMSICFGADDRH, machine_imsic_base),
            (SMSICFGADDR, SMSICFGADDRH, QEMU_VIRT_S_IMSIC_BASE),
        ] {
            let base_ppn = (imsic_base >> 12) & !((1usize << hart_index_bits) - 1);
            aplic.write::<u32>(address, base_ppn as u32);
            aplic.write::<u32>(
                address_high,
                ((base_ppn >> 32) as u32) | (hart_index_bits << MSICFGADDRH_LHXW_SHIFT),
            );
        }
    }

    // Clear domaincfg.IE: the M domain delivers nothing itself once every
    // source is delegated.
    aplic.write::<u32>(DOMAINCFG, 0);

    // Clear the enable bits for sources 0..=96 across the clrie words.
    let num_words = (QEMU_VIRT_APLIC_NUM_SOURCES + 1).div_ceil(32);
    for word in 0..num_words {
        aplic.write::<u32>(CLRIE + word * 4, u32::MAX);
    }

    // sourcecfg[i] configures interrupt source i + 1; source 0 does not
    // exist (AIA v1.0 §4.5, Table 4.1).
    for index in 0..QEMU_VIRT_APLIC_NUM_SOURCES {
        aplic.write::<u32>(SOURCECFG + index * 4, SOURCECFG_DELEGATE);
    }

    info!(
        "AIA: delegated M-level APLIC IRQs 1..={} to S-level child",
        QEMU_VIRT_APLIC_NUM_SOURCES
    );
    true
}
