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

use core::fmt;

use riscv::register::{self, Permission};

use runtime::hart::HartId;

/// Decides whether this hart leads the boot (designated in `DynamicInfo`,
/// or raced when absent).
fn is_selected_boot_hart(dynamic_info_address: usize) -> bool {
    use core::sync::atomic::{AtomicUsize, Ordering};
    static BOOT_HART_ID: AtomicUsize = AtomicUsize::new(usize::MAX);

    cfg_if::cfg_if! {
        if #[cfg(any(feature = "payload", feature = "jump"))] {
            let _ = dynamic_info_address;
            let selected_hart: Option<usize> = None;
        }
        else {
            let selected_hart = read_dynamic_info(dynamic_info_address)
                .ok()
                .map(|dynamic_info| dynamic_info.boot_hart);
        }
    }

    let claim_boot_hart = || {
        let hart_id = HartId::current()
            .expect("BUG: current hart exceeds Runtime capacity")
            .as_usize();
        match BOOT_HART_ID.compare_exchange(
            usize::MAX,
            hart_id,
            Ordering::AcqRel,
            Ordering::Acquire,
        ) {
            Ok(_) => true,
            Err(selected_hart) => selected_hart == hart_id,
        }
    };

    match selected_hart {
        Some(hart_id) => {
            if hart_id == usize::MAX {
                claim_boot_hart()
            } else {
                HartId::current()
                    .expect("BUG: current hart exceeds Runtime capacity")
                    .as_usize()
                    == hart_id
            }
        }
        // Without a readable DynamicInfo, race to elect a single boot hart.
        None => claim_boot_hart(),
    }
}

use core::arch::asm;
use core::ops::Range;

use runtime::boot::NextStage;

/// Boot information decoded from the previous-stage register envelope (a1/a2).
pub struct BootInfo {
    device_tree_address: usize,
    is_boot_hart: bool,
    platform_description: Option<runtime::PlatformDescription>,
    /// The `a2` `DynamicInfo` address, kept for the deferred `next_stage()`
    /// decode.
    dynamic_info_address: usize,
}

impl BootInfo {
    /// Decodes the entry handoff, electing a boot hart by race when
    /// `DynamicInfo` is unreadable.
    pub fn decode(
        device_tree: runtime::DeviceTreeHandoff,
        dynamic_info_address: usize,
    ) -> runtime::Result<Self> {
        let selection =
            resolve_boot_selection(device_tree.address().as_usize(), dynamic_info_address);
        let platform_description = if selection.is_boot_hart {
            Some(device_tree.claim(runtime::memory::PhysAddr::new(
                selection.device_tree_address,
            ))?)
        } else {
            None
        };
        Ok(Self {
            device_tree_address: selection.device_tree_address,
            is_boot_hart: selection.is_boot_hart,
            platform_description,
            dynamic_info_address,
        })
    }

    /// Returns whether this hart leads the boot.
    pub fn is_boot_hart(&self) -> bool {
        self.is_boot_hart
    }

    /// Returns the boot hart's validated Platform Description.
    pub fn take_platform_description(&mut self) -> Option<runtime::PlatformDescription> {
        self.platform_description.take()
    }

    /// Returns the next-stage handoff; `opaque` carries the unpatched
    /// device tree address. Must be called after the console is up:
    /// prints and stops on invalid `DynamicInfo`.
    pub fn next_stage(&self) -> NextStage {
        let (next_mode, start_address) = decode_next_stage(self.dynamic_info_address);
        NextStage {
            start_addr: start_address,
            next_mode,
            opaque: self.device_tree_address,
        }
    }
}

/// The local hart's boot role and its resolved device tree address.
struct BootSelection {
    device_tree_address: usize,
    is_boot_hart: bool,
}

#[cfg(all(feature = "fdt", not(feature = "payload")))]
const LINKED_FDT_PTR: *const u8 = raw_fdt.0.as_ptr();
#[cfg(all(feature = "fdt", feature = "payload"))]
const LINKED_FDT_PTR: *const u8 = payload::raw_fdt.0.as_ptr();
#[inline]
#[cfg(feature = "fdt")]
fn linked_fdt_address() -> usize {
    let address = LINKED_FDT_PTR as usize;
    // SAFETY: the empty asm is only an optimization barrier; it reads no
    // memory, uses no stack, and preserves flags, so that the runtime
    // (post-relocation) address of the linker-script-placed `.fdt` section
    // is used instead of a constant-folded link-time address.
    unsafe { core::arch::asm!("", options(nomem, nostack, preserves_flags)) };
    address
}

/// Resolves this hart's boot role and the device tree address.
#[allow(unused_mut, unused_assignments)]
fn resolve_boot_selection(
    entry_device_tree_address: usize,
    dynamic_info_address: usize,
) -> BootSelection {
    let is_boot_hart = is_selected_boot_hart(dynamic_info_address);

    let mut device_tree_address = entry_device_tree_address;

    #[cfg(feature = "fdt")]
    {
        device_tree_address = linked_fdt_address();
    }

    BootSelection {
        device_tree_address,
        is_boot_hart,
    }
}

static mut FIRMWARE_START_ADDRESS: usize = 0;
static mut FIRMWARE_END_ADDRESS: usize = 0;
static mut FIRMWARE_RODATA_START_ADDRESS: usize = 0;
static mut FIRMWARE_RODATA_END_ADDRESS: usize = 0;

/// Programs one of the first sixteen PMP entries, accounting for XLEN.
///
/// # Safety
/// Called during M-mode initialization on a hart with these PMP entries.
unsafe fn set_pmp_config(
    index: usize,
    range: register::Range,
    permission: Permission,
    locked: bool,
) {
    use riscv::register::*;
    let slot = index % size_of::<usize>();
    // SAFETY: each match arm addresses a valid slot in its native-width CSR.
    unsafe {
        match index / size_of::<usize>() {
            0 => pmpcfg0::set_pmp(slot, range, permission, locked),
            #[cfg(target_pointer_width = "32")]
            1 => pmpcfg1::set_pmp(slot, range, permission, locked),
            #[cfg(target_pointer_width = "32")]
            2 => pmpcfg2::set_pmp(slot, range, permission, locked),
            #[cfg(target_pointer_width = "32")]
            3 => pmpcfg3::set_pmp(slot, range, permission, locked),
            #[cfg(target_pointer_width = "64")]
            1 => pmpcfg2::set_pmp(slot, range, permission, locked),
            _ => unreachable!("firmware uses only the first sixteen PMP entries"),
        }
    }
}

/// Installs PMP entries isolating firmware memory from S-mode.
pub fn set_pmp(firmware_ram: &Range<usize>) {
    // SAFETY: M-mode PMP programming on this hart; the linker symbols and
    // memory bounds are asserted aligned below.
    unsafe {
        // [0..firmware_ram.start] RWX
        // [firmware_ram.start..firmware_start] RWX
        // [firmware_start..firmware_rodata_start] R
        // [firmware_rodata_start..firmware_rodata_end] NONE
        // [firmware_rodata_end..firmware_end] RW
        // [firmware_end..firmware_ram.end] RWX
        // [firmware_ram.end..INF] RWX
        use riscv::register::*;

        asm!("la {}, sbi_start", out(reg) FIRMWARE_START_ADDRESS, options(nomem));
        asm!("la {}, sbi_end", out(reg) FIRMWARE_END_ADDRESS, options(nomem));
        asm!(
            "la {}, sbi_rodata_start",
            out(reg) FIRMWARE_RODATA_START_ADDRESS,
            options(nomem)
        );
        asm!(
            "la {}, sbi_rodata_end",
            out(reg) FIRMWARE_RODATA_END_ADDRESS,
            options(nomem)
        );

        assert_eq!(firmware_ram.start & 0x3, 0);
        assert_eq!(firmware_ram.end & 0x3, 0);
        assert_eq!(FIRMWARE_START_ADDRESS & 0x3, 0);
        assert_eq!(FIRMWARE_END_ADDRESS & 0x3, 0);
        assert_eq!(FIRMWARE_RODATA_START_ADDRESS & 0x3, 0);
        assert_eq!(FIRMWARE_RODATA_END_ADDRESS & 0x3, 0);

        // Keep machine-level interrupt controllers inaccessible to S-mode
        // only when the IMSIC device retained them for firmware use.
        if crate::driver::ipi::uses_imsic()
            && crate::platform::board_info().is_qemu_virt()
            && let Some(imsic) = crate::platform::board_info().devices.interrupts.imsic()
        {
            const QEMU_VIRT_CLINT_BASE: usize = 0x0200_0000;
            const QEMU_VIRT_CLINT_SIZE: usize = 0x1_0000;

            let clint_start = crate::platform::board_info()
                .devices
                .interrupts
                .clint()
                .map(|description| description.resource().registers.start().as_usize())
                .unwrap_or(QEMU_VIRT_CLINT_BASE);
            let clint_end = clint_start + QEMU_VIRT_CLINT_SIZE;
            let aplic_registers = crate::platform::board_info()
                .devices
                .interrupts
                .machine_aplic()
                .expect("BUG: QEMU AIA setup requires a machine APLIC");
            let aplic_start = aplic_registers.resource().start().as_usize();
            let aplic_end = aplic_registers.resource().end().as_usize();
            let machine_imsic_start = imsic.resource().layout.machine_base.as_usize();
            let machine_imsic_end = imsic
                .resource()
                .hart_files
                .iter()
                .flatten()
                .map(|range| range.end().as_usize())
                .max()
                .unwrap_or(machine_imsic_start + 0x1000);

            set_pmp_config(0, Range::OFF, Permission::NONE, false);
            pmpaddr0::write(0);
            set_pmp_config(1, Range::TOR, Permission::RWX, false);
            pmpaddr1::write(clint_start >> 2);
            set_pmp_config(2, Range::TOR, Permission::NONE, false);
            pmpaddr2::write(clint_end >> 2);
            set_pmp_config(3, Range::TOR, Permission::RWX, false);
            pmpaddr3::write(aplic_start >> 2);
            set_pmp_config(4, Range::TOR, Permission::NONE, false);
            pmpaddr4::write(aplic_end >> 2);
            set_pmp_config(5, Range::TOR, Permission::RWX, false);
            pmpaddr5::write(machine_imsic_start >> 2);
            set_pmp_config(6, Range::TOR, Permission::NONE, false);
            pmpaddr6::write(machine_imsic_end >> 2);
            set_pmp_config(7, Range::TOR, Permission::RWX, false);
            pmpaddr7::write(firmware_ram.start >> 2);
            set_pmp_config(8, Range::TOR, Permission::RWX, false);
            pmpaddr8::write(FIRMWARE_START_ADDRESS >> 2);
            set_pmp_config(9, Range::TOR, Permission::R, false);
            pmpaddr9::write(FIRMWARE_RODATA_START_ADDRESS >> 2);
            set_pmp_config(10, Range::TOR, Permission::NONE, false);
            pmpaddr10::write(FIRMWARE_RODATA_END_ADDRESS >> 2);
            set_pmp_config(11, Range::TOR, Permission::RW, false);
            pmpaddr11::write(FIRMWARE_END_ADDRESS >> 2);
            set_pmp_config(12, Range::TOR, Permission::RWX, false);
            pmpaddr12::write(firmware_ram.end >> 2);
            set_pmp_config(13, Range::TOR, Permission::RWX, false);
            pmpaddr13::write(usize::MAX >> 2);
            return;
        }

        set_pmp_config(0, Range::OFF, Permission::NONE, false);
        pmpaddr0::write(0);
        set_pmp_config(1, Range::TOR, Permission::RWX, false);
        pmpaddr1::write(firmware_ram.start >> 2);
        set_pmp_config(2, Range::TOR, Permission::RWX, false);
        pmpaddr2::write(FIRMWARE_START_ADDRESS >> 2);
        set_pmp_config(3, Range::TOR, Permission::R, false);
        pmpaddr3::write(FIRMWARE_RODATA_START_ADDRESS >> 2);
        set_pmp_config(4, Range::TOR, Permission::NONE, false);
        pmpaddr4::write(FIRMWARE_RODATA_END_ADDRESS >> 2);
        set_pmp_config(5, Range::TOR, Permission::RW, false); // FIXME: should be `R`; `RW` temporarily allows S-mode DTB modification
        pmpaddr5::write(FIRMWARE_END_ADDRESS >> 2);
        set_pmp_config(6, Range::TOR, Permission::RWX, false);
        pmpaddr6::write(firmware_ram.end >> 2);
        if let Some(alias) = crate::platform::board_info()
            .memory
            .noncacheable_alias_offset
        {
            assert!(alias.is_power_of_two() && alias >= firmware_ram.end as u64);
            let start = (FIRMWARE_START_ADDRESS as u64)
                .checked_add(alias)
                .expect("BUG: V821 firmware alias start overflowed");
            let end = (FIRMWARE_END_ADDRESS as u64)
                .checked_add(alias)
                .expect("BUG: V821 firmware alias end overflowed");
            assert!(end >> 2 <= usize::MAX as u64);
            // Deny the firmware alias before permitting the wider physical address space.
            set_pmp_config(7, Range::OFF, Permission::NONE, false);
            pmpaddr7::write((start >> 2) as usize);
            set_pmp_config(8, Range::TOR, Permission::NONE, false);
            pmpaddr8::write((end >> 2) as usize);
            assert_eq!(pmpaddr8::read(), (end >> 2) as usize);
            set_pmp_config(9, Range::NAPOT, Permission::RWX, false);
            pmpaddr9::write(usize::MAX);
        } else {
            set_pmp_config(7, Range::TOR, Permission::RWX, false);
            pmpaddr7::write(usize::MAX >> 2);
        }
    }
}

/// Formats a PMP permission for logs.
#[repr(transparent)]
struct PermissionWrapper(pub Permission);

/// Formats a PMP range encoding for logs.
#[repr(transparent)]
struct RangeWrapper(pub register::Range);

impl fmt::Display for PermissionWrapper {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.pad(match self.0 {
            Permission::R => "R",
            Permission::W => "W",
            Permission::X => "X",
            Permission::RW => "RW",
            Permission::RX => "RX",
            Permission::WX => "WX",
            Permission::RWX => "RWX",
            Permission::NONE => "NONE",
        })
    }
}

impl fmt::Display for RangeWrapper {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.pad(match self.0 {
            register::Range::OFF => "OFF",
            register::Range::TOR => "TOR",
            register::Range::NA4 => "NA4",
            register::Range::NAPOT => "NAPOT",
        })
    }
}

/// Logs the active PMP configuration.
pub fn log_pmp_cfg(_firmware_ram: &Range<usize>) {
    use riscv::register::*;
    let pmp_config = |index: usize| {
        #[cfg(target_pointer_width = "32")]
        if index >= 4 {
            return if index < 8 {
                pmpcfg1::read().into_config(index - 4)
            } else {
                pmpcfg2::read().into_config(index - 8)
            };
        }
        #[cfg(target_pointer_width = "64")]
        if index >= 8 {
            return pmpcfg2::read().into_config(index - 8);
        }
        pmpcfg0::read().into_config(index)
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
        log_entry(N, pastey::paste! { [<pmpaddr ~N>]::read() });
    });
    if crate::platform::board_info()
        .memory
        .noncacheable_alias_offset
        .is_some()
    {
        seq_macro::seq!(N in 8..10 {
            log_entry(N, pastey::paste! { [<pmpaddr ~N>]::read() });
        });
    }
}

#[cfg(all(feature = "fdt", not(feature = "payload")))]
include!(concat!(env!("OUT_DIR"), "/generated_alignment.rs"));
#[cfg(all(feature = "fdt", not(feature = "payload")))]
include!(concat!(env!("OUT_DIR"), "/generated_fdt.rs"));
