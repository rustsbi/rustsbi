mod platform;
mod reset;
pub(crate) use platform::{BootHart, init_platform, initialize_secondary_hart};
pub(crate) use reset::{c907_reset_entry, k1_reset_entry};

cfg_if::cfg_if! {
    if #[cfg(feature = "payload")] {
        pub mod payload;
        use payload::decode_next_stage;
    } else if #[cfg(feature = "jump")] {
        pub mod jump;
        use jump::decode_next_stage;
    } else {
        pub mod dynamic;
        use dynamic::{decode_next_stage, read_dynamic_info};
    }
}

use alloc::vec::Vec;
use runtime::{
    DeviceTreeHandoff, Error, PlatformDescription, Result,
    boot::{NextStage, locate_firmware_image},
    memory::{PhysAddr, PhysAddrRange, RegRange},
    pmp::{self, PmpEntry, PmpPermission, PmpRange},
};
use spin::Once;

use crate::cfg;

/// Machine interrupt-controller MMIO ranges used by the PMP layout.
pub(crate) struct IrqMmio {
    pub(crate) clint_start: usize,
    pub(crate) aplic: RegRange,
    pub(crate) imsic_start: usize,
    pub(crate) imsic_end: usize,
}

struct PmpLayout {
    entries: Vec<(PmpRange, PmpPermission, usize)>,
    alias_end_index: Option<usize>,
}

static PMP: Once<PmpLayout> = Once::new();

unsafe extern "C" {
    static sbi_rodata_start: u8;
    static sbi_rodata_end: u8;
}

pub(crate) fn init_pmp(
    firmware_ram: PhysAddrRange,
    noncacheable_alias_offset: Option<u64>,
    irq_mmio: Option<IrqMmio>,
) {
    PMP.call_once(|| {
        use PmpPermission::{None as Deny, R, Rw, Rwx};
        use PmpRange::{Napot, Off, Tor};

        let firmware = locate_firmware_image()
            .expect("firmware bounds are available after topology initialization");
        let ram_start = firmware_ram.start().as_usize();
        let ram_end = firmware_ram.end().as_usize();
        let firmware_start = firmware.start().as_usize();
        let firmware_end = firmware.end().as_usize();
        let rodata_start = (&raw const sbi_rodata_start).addr();
        let rodata_end = (&raw const sbi_rodata_end).addr();

        let mut entries = Vec::new();
        entries.push((Off, Deny, 0));
        if let Some(mmio) = &irq_mmio {
            const QEMU_VIRT_CLINT_SIZE: usize = 0x1_0000;
            entries.extend([
                (Tor, Rwx, mmio.clint_start >> 2),
                (Tor, Deny, (mmio.clint_start + QEMU_VIRT_CLINT_SIZE) >> 2),
                (Tor, Rwx, mmio.aplic.start().as_usize() >> 2),
                (Tor, Deny, mmio.aplic.end().as_usize() >> 2),
                (Tor, Rwx, mmio.imsic_start >> 2),
                (Tor, Deny, mmio.imsic_end >> 2),
            ]);
        }

        for (end, permission) in [
            (ram_start, Rwx),
            (firmware_start, Rwx),
            (rodata_start, R),
            (rodata_end, Deny),
            // The handoff DTB in this region must remain writable by S-mode.
            (firmware_end, Rw),
            (ram_end, Rwx),
        ] {
            assert_eq!(end & 3, 0);
            entries.push((Tor, permission, end >> 2));
        }

        let mut alias_end_index = None;
        if let Some(alias) = noncacheable_alias_offset.filter(|_| irq_mmio.is_none()) {
            assert!(alias.is_power_of_two() && alias >= ram_end as u64);
            let start = (firmware_start as u64)
                .checked_add(alias)
                .expect("BUG: V821 firmware alias start overflowed");
            let end = (firmware_end as u64)
                .checked_add(alias)
                .expect("BUG: V821 firmware alias end overflowed");
            assert!(end >> 2 <= usize::MAX as u64);
            entries.push((Off, Deny, (start >> 2) as usize));
            alias_end_index = Some(entries.len());
            entries.push((Tor, Deny, (end >> 2) as usize));
            entries.push((Napot, Rwx, usize::MAX));
        } else {
            entries.push((Tor, Rwx, usize::MAX >> 2));
        }
        PmpLayout {
            entries,
            alias_end_index,
        }
    });
}

/// Boot information decoded from the previous-stage register envelope (`a1`/`a2`).
pub(crate) struct BootInfo {
    /// The unmodified device-tree address received in `a1`.
    device_tree_address: usize,
    is_boot_hart: bool,
    platform_description: Option<PlatformDescription>,
    /// The `DynamicInfo` address received in `a2`.
    ///
    /// Used to decode the next-stage handoff.
    dynamic_info_address: usize,
}

impl BootInfo {
    /// Decodes a boot candidate's handoff under the entry bootstrap lock.
    ///
    /// Without a designated hart, the first enabled candidate leads boot.
    pub(crate) fn decode(
        current_hart: usize,
        device_tree: DeviceTreeHandoff,
        dynamic_info_address: usize,
    ) -> Result<Self> {
        let designated_hart = designated_boot_hart(dynamic_info_address);
        // Only the designated boot hart's FDT belongs to platform discovery.
        if designated_hart.is_some_and(|hart| hart != current_hart) {
            return Ok(Self::secondary(device_tree, dynamic_info_address));
        }
        let entry_device_tree_address = device_tree.address().as_usize();
        let source_device_tree_address = resolve_device_tree_address(entry_device_tree_address);
        let destination = match cfg::FDT_HANDOFF {
            Some((address, capacity)) => PhysAddrRange::from_start_len(
                PhysAddr::new(usize::try_from(address).map_err(|_| Error::Overflow)?),
                usize::try_from(capacity).map_err(|_| Error::Overflow)?,
            )?,
            // SAFETY: Without an explicit destination, entry requires an incoming
            // FDT with exclusive writable storage for totalsize + 1024 bytes.
            None => unsafe { device_tree.incoming_destination()? },
        };
        // SAFETY:
        // 1. The loader or linked FDT describes this board's RAM, reservations
        //    and MMIO. Its source stays readable and unchanged until handoff consumes it.
        // 2. The destination is exclusively writable and remains live until
        //    the next stage finishes using it.
        // 3. `BOOT_LOCK` serializes candidate claims until the boot owner is selected.
        let platform =
            unsafe { device_tree.claim(PhysAddr::new(source_device_tree_address), destination)? };
        let is_boot_hart = if designated_hart.is_some() {
            // Runtime validates this designated hart's membership before
            // publishing the topology or releasing secondary harts.
            true
        } else {
            platform.inspect(|view| {
                let mut enabled = false;
                for hart_id in view.hart_ids()? {
                    enabled |= hart_id? == current_hart;
                }
                Ok(enabled)
            })?
        };
        Ok(Self {
            device_tree_address: entry_device_tree_address,
            is_boot_hart,
            platform_description: is_boot_hart.then_some(platform),
            dynamic_info_address,
        })
    }

    /// Constructs a secondary handoff without reading the boot hart's FDT.
    pub(crate) fn secondary(device_tree: DeviceTreeHandoff, dynamic_info_address: usize) -> Self {
        Self {
            device_tree_address: device_tree.address().as_usize(),
            is_boot_hart: false,
            platform_description: None,
            dynamic_info_address,
        }
    }

    pub(crate) fn platform_description(&self) -> &PlatformDescription {
        self.platform_description
            .as_ref()
            .expect("boot hart owns its platform description")
    }

    /// Returns the dynamic boot information's storage range, if present.
    ///
    /// Jump and payload modes return `None`. Stack placement checks this range
    /// independently of the device tree's storage.
    pub(crate) fn dynamic_info_range(&self) -> Result<Option<PhysAddrRange>> {
        #[cfg(not(any(feature = "payload", feature = "jump")))]
        {
            PhysAddrRange::from_start_len(
                PhysAddr::new(self.dynamic_info_address),
                size_of::<dynamic::DynamicInfo>(),
            )
            .map(Some)
        }
        #[cfg(any(feature = "payload", feature = "jump"))]
        {
            Ok(None)
        }
    }

    /// Returns whether this hart leads the boot.
    pub(crate) fn is_boot_hart(&self) -> bool {
        self.is_boot_hart
    }

    /// Returns the boot hart's validated Platform Description.
    pub(crate) fn take_platform_description(&mut self) -> Option<PlatformDescription> {
        self.platform_description.take()
    }

    /// Returns the next-stage handoff with the entry device-tree address in
    /// `opaque`. Platform initialization replaces it with the prepared destination.
    /// Invalid `DynamicInfo` stops boot.
    pub(crate) fn next_stage(&self) -> NextStage {
        let (next_mode, start_address) = decode_next_stage(self.dynamic_info_address);
        NextStage {
            start_addr: start_address,
            next_mode,
            opaque: self.device_tree_address,
        }
    }
}

fn designated_boot_hart(dynamic_info_address: usize) -> Option<usize> {
    cfg_if::cfg_if! {
        if #[cfg(any(feature = "payload", feature = "jump"))] {
            let _ = dynamic_info_address;
            None
        }
        else {
            read_dynamic_info(dynamic_info_address)
                .ok()
                .map(|dynamic_info| dynamic_info.boot_hart)
                .filter(|hart_id| *hart_id != usize::MAX)
        }
    }
}

#[inline]
#[cfg(feature = "fdt")]
fn linked_fdt_address() -> usize {
    #[cfg(not(feature = "payload"))]
    let address = raw_fdt.0.as_ptr() as usize;
    #[cfg(feature = "payload")]
    let address = payload::raw_fdt.0.as_ptr() as usize;
    // SAFETY: the empty asm is only an optimization barrier; it reads no
    // memory, uses no stack, and preserves flags, so that the runtime
    // (post-relocation) address of the linker-script-placed `.fdt` section
    // is used instead of a constant-folded link-time address.
    unsafe { core::arch::asm!("", options(nomem, nostack, preserves_flags)) };
    address
}

/// Resolves the device tree address selected by the firmware build.
#[allow(unused_mut, unused_assignments)]
fn resolve_device_tree_address(entry_device_tree_address: usize) -> usize {
    let mut device_tree_address = entry_device_tree_address;

    #[cfg(feature = "fdt")]
    {
        device_tree_address = linked_fdt_address();
    }

    device_tree_address
}

/// Applies the published RAM, firmware and device permissions on this hart.
///
/// # Safety
///
/// 1. Run in M-mode during hart startup with Runtime trap recovery ready,
///    `MPRV` clear and local interrupts disabled.
/// 2. Own the unlocked PMP entries needed for the published layout.
///    The hart must support their address ranges and permissions.
/// 3. Keep all live firmware accesses valid during intermediate writes,
///    including changes to the lower bound of a following TOR entry.
pub(crate) unsafe fn set_pmp() {
    let layout = PMP
        .get()
        .expect("BUG: PMP layout used before initialization");
    for (slot, &(range, permission, address)) in layout.entries.iter().enumerate() {
        let index = u8::try_from(slot).expect("PMP index fits in u8");
        let entry = PmpEntry {
            range,
            permission,
            locked: false,
        };
        assert!(
            // SAFETY:
            // 1. Startup owns the PMP entries and their configuration words on this hart.
            // 2. The caller guarantees that intermediate TOR bounds preserve live accesses.
            // 3. The translation fence below runs before lower-mode entry.
            unsafe { pmp::write_entry(index, entry, address) },
            "invalid PMP entry index {index}"
        );
        if layout.alias_end_index == Some(slot) {
            // Both TOR bounds and the deny entry must hold before the final grant.
            assert_eq!(pmp::read_addr(index - 1), Some(layout.entries[slot - 1].2));
            assert_eq!(pmp::read_entry(index), Some((entry, address)));
        }
    }
    // SAFETY: Startup still owns the hart with recovery active, interrupts masked,
    // MPRV clear and no lower-mode trap context to preserve.
    unsafe { runtime::fence::synchronize_translations() };
}

fn permission_str(permission: PmpPermission) -> &'static str {
    match permission {
        PmpPermission::R => "R",
        PmpPermission::W => "W",
        PmpPermission::X => "X",
        PmpPermission::Rw => "RW",
        PmpPermission::Rx => "RX",
        PmpPermission::Wx => "WX",
        PmpPermission::Rwx => "RWX",
        PmpPermission::None => "NONE",
    }
}

fn range_str(range: PmpRange) -> &'static str {
    match range {
        PmpRange::Off => "OFF",
        PmpRange::Tor => "TOR",
        PmpRange::Na4 => "NA4",
        PmpRange::Napot => "NAPOT",
    }
}

/// Logs the active PMP configuration.
pub fn log_pmp_cfg() {
    let layout = PMP
        .get()
        .expect("BUG: PMP layout used before initialization");
    let count = u8::try_from(layout.entries.len()).expect("PMP entry count fits in u8");

    info!("PMP Configuration");
    info!(
        "{:<5} {:<10} {:<15} {:<30}",
        "PMP", "Range", "Permission", "Address"
    );

    for index in 0..count {
        if let Some((entry, address)) = pmp::read_entry(index) {
            info!(
                "{:<5} {:<10} {:<15} 0x{:016x}",
                index,
                range_str(entry.range),
                permission_str(entry.permission),
                (address as u64) << 2,
            );
        }
    }
}

#[cfg(all(feature = "fdt", not(feature = "payload")))]
include!(concat!(env!("OUT_DIR"), "/generated_alignment.rs"));
#[cfg(all(feature = "fdt", not(feature = "payload")))]
include!(concat!(env!("OUT_DIR"), "/generated_fdt.rs"));
