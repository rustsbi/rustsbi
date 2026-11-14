mod warm;
pub(crate) use warm::warm_entry;

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
use core::{arch::asm, fmt};
use runtime::{
    DeviceTreeHandoff, Error, PlatformDescription, Result,
    boot::NextStage,
    memory::{PhysAddr, PhysAddrRange, RegRange, locate_firmware_image},
};
use spin::Once;

use csr::machine::{PmpAddress, PmpConfiguration, PmpEntry, PmpPermission, PmpRange};

use crate::cfg;

/// Machine interrupt-controller MMIO ranges used by the PMP layout.
pub(crate) struct IrqMmio {
    pub(crate) clint_start: usize,
    pub(crate) aplic: RegRange,
    pub(crate) imsic_start: usize,
    pub(crate) imsic_end: usize,
}

/// MMIO window length of the machine interrupt controller shared by PMP
/// installation and entry-address validation.
const QEMU_VIRT_CLINT_SIZE: usize = 0x1_0000;

struct PmpLayout {
    firmware_ram: PhysAddrRange,
    entry_ram_ranges: Vec<PhysAddrRange>,
    noncacheable_alias_offset: Option<u64>,
    irq_mmio: Option<IrqMmio>,
}

static PMP: Once<PmpLayout> = Once::new();

pub(crate) fn init_pmp(
    firmware_ram: PhysAddrRange,
    entry_ram_ranges: Vec<PhysAddrRange>,
    noncacheable_alias_offset: Option<u64>,
    irq_mmio: Option<IrqMmio>,
) {
    PMP.call_once(|| PmpLayout {
        firmware_ram,
        entry_ram_ranges,
        noncacheable_alias_offset,
        irq_mmio,
    });
}

/// Returns whether an S-mode entry address may be handed to HSM start or
/// non-retentive resume.
///
/// The address must lie in a RAM range described by the platform FDT and must
/// not fall inside the firmware image, its non-cacheable alias, or the
/// machine interrupt-controller MMIO windows that the current PMP layout
/// keeps inaccessible to S-mode. All ranges are half-open `[start, end)`.
///
/// The check reads only immutable platform descriptions and integer range
/// arithmetic; it never dereferences the entry address. Reserved RAM and
/// embedded payloads outside the firmware image stay valid entries.
pub(crate) fn is_valid_entry_address(address: usize) -> bool {
    let Some(pmp) = PMP.get() else {
        return false;
    };
    if !pmp
        .entry_ram_ranges
        .iter()
        .any(|range| range.start().as_usize() <= address && address < range.end().as_usize())
    {
        return false;
    }
    // The firmware image includes the runtime extension stacks.
    let Ok(firmware) = locate_firmware_image() else {
        return false;
    };
    if firmware.start().as_usize() <= address && address < firmware.end().as_usize() {
        return false;
    }
    if let Some(alias) = pmp.noncacheable_alias_offset {
        let Some(alias_start) = (firmware.start().as_usize() as u64).checked_add(alias) else {
            return false;
        };
        let Some(alias_end) = (firmware.end().as_usize() as u64).checked_add(alias) else {
            return false;
        };
        if alias_start <= address as u64 && (address as u64) < alias_end {
            return false;
        }
    }
    if let Some(irq_mmio) = &pmp.irq_mmio {
        let Some(clint_end) = irq_mmio.clint_start.checked_add(QEMU_VIRT_CLINT_SIZE) else {
            return false;
        };
        if (irq_mmio.clint_start..clint_end).contains(&address) {
            return false;
        }
        if (irq_mmio.aplic.start().as_usize()..irq_mmio.aplic.end().as_usize()).contains(&address) {
            return false;
        }
        if (irq_mmio.imsic_start..irq_mmio.imsic_end).contains(&address) {
            return false;
        }
    }
    true
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
        // 1. Entry guarantees a readable, stable source (linked with `fdt`).
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

/// Programs one of the first sixteen PMP entries, accounting for XLEN.
///
/// # Safety
/// The caller must satisfy [`PmpConfiguration::set`]'s safety requirements
/// for the selected entry and its new configuration.
unsafe fn set_pmp_config(index: usize, range: PmpRange, permission: PmpPermission, locked: bool) {
    let slot = index % size_of::<usize>();
    let entry = PmpEntry {
        range,
        permission,
        locked,
    };
    // SAFETY: The caller upholds the PMP configuration requirements. Each
    // match arm selects a native-width CSR and a slot within that CSR.
    unsafe {
        match index / size_of::<usize>() {
            0 => PmpConfiguration::<0>::set(slot, entry),
            #[cfg(target_pointer_width = "32")]
            1 => PmpConfiguration::<1>::set(slot, entry),
            #[cfg(target_pointer_width = "32")]
            2 => PmpConfiguration::<2>::set(slot, entry),
            #[cfg(target_pointer_width = "32")]
            3 => PmpConfiguration::<3>::set(slot, entry),
            #[cfg(target_pointer_width = "64")]
            1 => PmpConfiguration::<2>::set(slot, entry),
            _ => unreachable!("firmware uses only the first sixteen PMP entries"),
        }
    }
}

/// Installs PMP entries isolating firmware memory from S-mode.
pub fn set_pmp() {
    let pmp = PMP
        .get()
        .expect("BUG: PMP layout used before initialization");
    let range = pmp.firmware_ram;
    let firmware_ram = range.start().as_usize()..range.end().as_usize();
    // FIXME: These PMP updates do not synchronize address-translation caches
    // as required when page-based virtual memory is implemented.
    // See RISC-V Privileged Architecture, Section 3.7.2:
    // <https://docs.riscv.org/reference/isa/v20240411/_attachments/riscv-privileged.pdf>.
    // SAFETY:
    // 1. Startup owns this hart's PMP state in M-mode before lower-mode entry.
    // 2. The platform implements the PMP entries and encodings used below.
    // 3. Platform initialization selected a RAM bank containing the firmware.
    //    Section addresses come from the linker and are checked for alignment.
    unsafe {
        // [0..firmware_ram.start] RWX
        // [firmware_ram.start..firmware_start] RWX
        // [firmware_start..firmware_rodata_start] R
        // [firmware_rodata_start..firmware_rodata_end] NONE
        // [firmware_rodata_end..firmware_end] RW
        // [firmware_end..firmware_ram.end] RWX
        // [firmware_ram.end..INF] RWX
        let firmware_start_address: usize;
        let firmware_rodata_start_address: usize;
        let firmware_rodata_end_address: usize;
        asm!("la {}, sbi_start", out(reg) firmware_start_address, options(nomem));
        let firmware_end_address = locate_firmware_image()
            .expect("firmware bounds are available after topology initialization")
            .end()
            .as_usize();
        asm!(
            "la {}, sbi_rodata_start",
            out(reg) firmware_rodata_start_address,
            options(nomem)
        );
        asm!(
            "la {}, sbi_rodata_end",
            out(reg) firmware_rodata_end_address,
            options(nomem)
        );

        assert_eq!(firmware_ram.start & 0x3, 0);
        assert_eq!(firmware_ram.end & 0x3, 0);
        assert_eq!(firmware_start_address & 0x3, 0);
        assert_eq!(firmware_end_address & 0x3, 0);
        assert_eq!(firmware_rodata_start_address & 0x3, 0);
        assert_eq!(firmware_rodata_end_address & 0x3, 0);

        // Keep machine-level interrupt controllers inaccessible to S-mode
        // only when the IMSIC device retained them for firmware use.
        if let Some(irq_mmio) = &pmp.irq_mmio {
            let clint_start = irq_mmio.clint_start;
            let clint_end = clint_start + QEMU_VIRT_CLINT_SIZE;
            let aplic_start = irq_mmio.aplic.start().as_usize();
            let aplic_end = irq_mmio.aplic.end().as_usize();
            let machine_imsic_start = irq_mmio.imsic_start;
            let machine_imsic_end = irq_mmio.imsic_end;

            set_pmp_config(0, PmpRange::Off, PmpPermission::None, false);
            PmpAddress::<0>::write(0);
            set_pmp_config(1, PmpRange::Tor, PmpPermission::Rwx, false);
            PmpAddress::<1>::write(clint_start >> 2);
            set_pmp_config(2, PmpRange::Tor, PmpPermission::None, false);
            PmpAddress::<2>::write(clint_end >> 2);
            set_pmp_config(3, PmpRange::Tor, PmpPermission::Rwx, false);
            PmpAddress::<3>::write(aplic_start >> 2);
            set_pmp_config(4, PmpRange::Tor, PmpPermission::None, false);
            PmpAddress::<4>::write(aplic_end >> 2);
            set_pmp_config(5, PmpRange::Tor, PmpPermission::Rwx, false);
            PmpAddress::<5>::write(machine_imsic_start >> 2);
            set_pmp_config(6, PmpRange::Tor, PmpPermission::None, false);
            PmpAddress::<6>::write(machine_imsic_end >> 2);
            set_pmp_config(7, PmpRange::Tor, PmpPermission::Rwx, false);
            PmpAddress::<7>::write(firmware_ram.start >> 2);
            set_pmp_config(8, PmpRange::Tor, PmpPermission::Rwx, false);
            PmpAddress::<8>::write(firmware_start_address >> 2);
            set_pmp_config(9, PmpRange::Tor, PmpPermission::R, false);
            PmpAddress::<9>::write(firmware_rodata_start_address >> 2);
            set_pmp_config(10, PmpRange::Tor, PmpPermission::None, false);
            PmpAddress::<10>::write(firmware_rodata_end_address >> 2);
            set_pmp_config(11, PmpRange::Tor, PmpPermission::Rw, false);
            PmpAddress::<11>::write(firmware_end_address >> 2);
            set_pmp_config(12, PmpRange::Tor, PmpPermission::Rwx, false);
            PmpAddress::<12>::write(firmware_ram.end >> 2);
            set_pmp_config(13, PmpRange::Tor, PmpPermission::Rwx, false);
            PmpAddress::<13>::write(usize::MAX >> 2);
            return;
        }

        set_pmp_config(0, PmpRange::Off, PmpPermission::None, false);
        PmpAddress::<0>::write(0);
        set_pmp_config(1, PmpRange::Tor, PmpPermission::Rwx, false);
        PmpAddress::<1>::write(firmware_ram.start >> 2);
        set_pmp_config(2, PmpRange::Tor, PmpPermission::Rwx, false);
        PmpAddress::<2>::write(firmware_start_address >> 2);
        set_pmp_config(3, PmpRange::Tor, PmpPermission::R, false);
        PmpAddress::<3>::write(firmware_rodata_start_address >> 2);
        set_pmp_config(4, PmpRange::Tor, PmpPermission::None, false);
        PmpAddress::<4>::write(firmware_rodata_end_address >> 2);
        set_pmp_config(5, PmpRange::Tor, PmpPermission::Rw, false); // FIXME: should be `R`; `RW` temporarily allows S-mode DTB modification.
        PmpAddress::<5>::write(firmware_end_address >> 2);
        set_pmp_config(6, PmpRange::Tor, PmpPermission::Rwx, false);
        PmpAddress::<6>::write(firmware_ram.end >> 2);
        if let Some(alias) = pmp.noncacheable_alias_offset {
            assert!(alias.is_power_of_two() && alias >= firmware_ram.end as u64);
            let start = (firmware_start_address as u64)
                .checked_add(alias)
                .expect("BUG: V821 firmware alias start overflowed");
            let end = (firmware_end_address as u64)
                .checked_add(alias)
                .expect("BUG: V821 firmware alias end overflowed");
            assert!(end >> 2 <= usize::MAX as u64);
            // Deny the firmware alias before permitting the wider physical address space.
            set_pmp_config(7, PmpRange::Off, PmpPermission::None, false);
            PmpAddress::<7>::write((start >> 2) as usize);
            set_pmp_config(8, PmpRange::Tor, PmpPermission::None, false);
            PmpAddress::<8>::write((end >> 2) as usize);
            assert_eq!(PmpAddress::<8>::read(), (end >> 2) as usize);
            set_pmp_config(9, PmpRange::Napot, PmpPermission::Rwx, false);
            PmpAddress::<9>::write(usize::MAX);
        } else {
            set_pmp_config(7, PmpRange::Tor, PmpPermission::Rwx, false);
            PmpAddress::<7>::write(usize::MAX >> 2);
        }
    }
}

/// A PMP permission formatter for logs.
#[repr(transparent)]
struct PermissionWrapper(pub PmpPermission);

/// A PMP range formatter for logs.
#[repr(transparent)]
struct RangeWrapper(pub PmpRange);

impl fmt::Display for PermissionWrapper {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.pad(match self.0 {
            PmpPermission::R => "R",
            PmpPermission::W => "W",
            PmpPermission::X => "X",
            PmpPermission::Rw => "RW",
            PmpPermission::Rx => "RX",
            PmpPermission::Wx => "WX",
            PmpPermission::Rwx => "RWX",
            PmpPermission::None => "NONE",
        })
    }
}

impl fmt::Display for RangeWrapper {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.pad(match self.0 {
            PmpRange::Off => "OFF",
            PmpRange::Tor => "TOR",
            PmpRange::Na4 => "NA4",
            PmpRange::Napot => "NAPOT",
        })
    }
}

/// Logs the active PMP configuration.
pub fn log_pmp_cfg() {
    let pmp_config = |index: usize| {
        #[cfg(target_pointer_width = "32")]
        if index >= 4 {
            let (word, slot) = if index < 8 {
                (PmpConfiguration::<1>::read(), index - 4)
            } else {
                (PmpConfiguration::<2>::read(), index - 8)
            };
            return PmpEntry::from_bits((word >> (slot * 8)) as u8);
        }
        #[cfg(target_pointer_width = "64")]
        if index >= 8 {
            return PmpEntry::from_bits((PmpConfiguration::<2>::read() >> ((index - 8) * 8)) as u8);
        }
        PmpEntry::from_bits((PmpConfiguration::<0>::read() >> (index * 8)) as u8)
    };

    let get_pmp_range = |index: usize| -> RangeWrapper { RangeWrapper(pmp_config(index).range) };
    let get_pmp_permission =
        |index: usize| -> PermissionWrapper { PermissionWrapper(pmp_config(index).permission) };
    info!("PMP Configuration");

    info!(
        "{:<5} {:<10} {:<15} {:<30}",
        "PMP", "Range", "Permission", "Address"
    );

    let log_entry = |index, address: usize| {
        info!(
            "{:<5} {:<10} {:<15} 0x{:016x}",
            index,
            get_pmp_range(index),
            get_pmp_permission(index),
            (address as u64) << 2,
        );
    };
    seq_macro::seq!(N in 0..8 {
        log_entry(N, PmpAddress::<N>::read());
    });
    if PMP
        .get()
        .expect("BUG: PMP layout used before initialization")
        .noncacheable_alias_offset
        .is_some()
    {
        seq_macro::seq!(N in 8..10 {
            log_entry(N, PmpAddress::<N>::read());
        });
    }
}

#[cfg(all(feature = "fdt", not(feature = "payload")))]
include!(concat!(env!("OUT_DIR"), "/generated_alignment.rs"));
#[cfg(all(feature = "fdt", not(feature = "payload")))]
include!(concat!(env!("OUT_DIR"), "/generated_fdt.rs"));
