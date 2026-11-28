mod platform;
mod reset;
pub(crate) use platform::{BootHart, init_platform, initialize_secondary_hart};
pub(crate) use reset::{spacemit_k1_reset_entry, thead_c907_reset_entry};

#[cfg(not(any(feature = "payload", feature = "jump")))]
mod dynamic;
#[cfg(feature = "payload")]
mod payload;

use alloc::vec::Vec;
use riscv_csr_spec::standard::isa::values::status::Privilege;
use runtime::{
    DeviceTreeHandoff, Error, PlatformDescription, Result,
    boot::locate_firmware_image,
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

static PMP: Once<Vec<(PmpRange, PmpPermission, usize)>> = Once::new();

/// Checks the execute bit of the first matching entry in the shared hart PMP layout.
pub(crate) fn permits_instruction_fetch(address: PhysAddr) -> bool {
    let Some(layout) = PMP.get() else {
        return false;
    };
    let word = address.as_usize() >> 2;
    let mut lower = 0;
    for &(mode, permission, encoded) in layout {
        let matches = match mode {
            PmpRange::Off => false,
            PmpRange::Tor => (lower..encoded).contains(&word),
            PmpRange::Na4 => word == encoded,
            PmpRange::Napot => {
                // Ignore the trailing ones and the first zero of the NAPOT encoding.
                let ignored = encoded ^ encoded.wrapping_add(1);
                word & !ignored == encoded & !ignored
            }
        };
        if matches {
            return permission as u8 & PmpPermission::X as u8 != 0;
        }
        lower = encoded;
    }
    false
}

unsafe extern "C" {
    static sbi_rodata_start: u8;
}

pub(crate) fn init_pmp(
    firmware_ram: PhysAddrRange,
    noncacheable_alias_offset: Option<u64>,
    irq_mmio: Option<IrqMmio>,
) {
    PMP.call_once(|| {
        use PmpPermission::{None as Deny, R, Rwx};
        use PmpRange::{Napot, Off, Tor};

        let firmware = locate_firmware_image()
            .expect("firmware bounds are available after topology initialization");
        let firmware_start = firmware.start().as_usize();
        let firmware_end = firmware.end().as_usize();
        let rodata_start = (&raw const sbi_rodata_start).addr();

        assert_eq!((firmware_start | rodata_start | firmware_end) & 3, 0);
        let mut entries = alloc::vec![
            (Off, Deny, firmware_start >> 2),
            (Tor, R, rodata_start >> 2),
            (Tor, Deny, firmware_end >> 2),
        ];
        if let Some(mmio) = irq_mmio {
            const QEMU_VIRT_CLINT_SIZE: usize = 0x1_0000;
            entries.extend([
                (Off, Deny, mmio.clint_start >> 2),
                (Tor, Deny, (mmio.clint_start + QEMU_VIRT_CLINT_SIZE) >> 2),
                (Off, Deny, mmio.aplic.start().as_usize() >> 2),
                (Tor, Deny, mmio.aplic.end().as_usize() >> 2),
                (Off, Deny, mmio.imsic_start >> 2),
                (Tor, Deny, mmio.imsic_end >> 2),
            ]);
        }

        if let Some(alias) = noncacheable_alias_offset {
            assert!(alias.is_power_of_two() && alias >= firmware_ram.end().as_usize() as u64);
            let start = (firmware_start as u64)
                .checked_add(alias)
                .expect("BUG: V821 firmware alias start overflowed");
            let end = (firmware_end as u64)
                .checked_add(alias)
                .expect("BUG: V821 firmware alias end overflowed");
            assert!(end >> 2 <= usize::MAX as u64);
            entries.push((Off, Deny, (start >> 2) as usize));
            entries.push((Tor, Deny, (end >> 2) as usize));
            entries.push((Napot, Rwx, usize::MAX));
        } else {
            entries.push((Off, Deny, 0));
            entries.push((Tor, Rwx, usize::MAX >> 2));
        }
        entries
    });
}

/// Boot information decoded from the previous-stage register envelope (`a1`/`a2`).
pub(crate) struct BootInfo {
    platform_description: Option<PlatformDescription>,
    #[cfg(not(any(feature = "payload", feature = "jump")))]
    dynamic: Result<dynamic::DynamicInfo, dynamic::ReadError>,
}

impl BootInfo {
    /// Captures the loader's handoff for this hart.
    ///
    /// # Safety
    ///
    /// In dynamic mode, `a2` must satisfy the loader's record validity contract
    /// for the duration of this call. Its target must be trusted to execute in
    /// the requested mode and preserve live firmware memory if entered in M-mode.
    pub(crate) unsafe fn from_entry(_a2: usize) -> Self {
        Self {
            platform_description: None,
            #[cfg(not(any(feature = "payload", feature = "jump")))]
            // SAFETY: Entry supplies the loader's stable, version-sized record.
            dynamic: unsafe { dynamic::DynamicInfo::read(_a2) },
        }
    }

    /// Claims the platform on the designated or first enabled hart under the entry lock.
    pub(crate) fn select_boot_hart(
        &mut self,
        current_hart: usize,
        device_tree: DeviceTreeHandoff,
    ) -> Result<bool> {
        let designated_hart = cfg_select! {
            any(feature = "payload", feature = "jump") => None::<usize>,
            _ => self.dynamic.as_ref().ok().and_then(|info| info.boot_hart),
        };
        // Only the designated boot hart's FDT belongs to platform discovery.
        if designated_hart.is_some_and(|hart| hart != current_hart) {
            return Ok(false);
        }
        let source_device_tree_address = cfg_select! {
            feature = "fdt" => linked_fdt_address(),
            _ => device_tree.address().as_usize(),
        };
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
        self.platform_description = is_boot_hart.then_some(platform);
        Ok(is_boot_hart)
    }

    pub(crate) fn platform_description_mut(&mut self) -> &mut PlatformDescription {
        self.platform_description
            .as_mut()
            .expect("boot hart owns its platform description")
    }

    /// Returns the dynamic boot information's storage range, if present.
    ///
    /// Jump and payload modes return `None`. Stack placement checks this range
    /// independently of the device tree's storage.
    pub(crate) fn dynamic_info_range(&self) -> Option<PhysAddrRange> {
        cfg_select! {
            any(feature = "payload", feature = "jump") => None,
            _ => self.dynamic.ok().map(|info| info.storage),
        }
    }

    /// Transfers the validated platform to the boot hart's initialization path.
    pub(crate) fn take_platform_description(&mut self) -> Option<PlatformDescription> {
        self.platform_description.take()
    }

    /// Returns the next-stage privilege and entry address.
    pub(crate) fn next_stage(&self) -> (Privilege, usize) {
        cfg_select! {
            feature = "payload" => (Privilege::Supervisor, payload::payload_image.address().as_usize()),
            feature = "jump" => (Privilege::Supervisor, cfg::JUMP_ADDRESS),
            _ => self.dynamic.unwrap_or_else(|err| err.fail()).next_stage(),
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
    for (slot, &(range, permission, address)) in layout.iter().enumerate() {
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
        if range == PmpRange::Tor && permission == PmpPermission::None {
            // Verify both bounds before relying on a protected TOR range.
            assert_eq!(pmp::read_addr(index - 1), Some(layout[slot - 1].2));
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
    let count = u8::try_from(layout.len()).expect("PMP entry count fits in u8");

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
