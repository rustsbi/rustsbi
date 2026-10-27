//! Per-hart firmware memory protection and PMP reporting.

use core::{arch::asm, fmt, ops::Range};
use riscv::register::{self, Permission};
use runtime::{ipi::IpiDelivery, memory::locate_firmware_image};

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
pub(crate) fn set_pmp(firmware_ram: &Range<usize>) {
    let (firmware_start, rodata_start, rodata_end): (usize, usize, usize);
    // SAFETY: Every register written below belongs to this hart; address
    // discovery uses ordinary local variables rather than shared mutable state.
    unsafe {
        // [0..firmware_ram.start] RWX
        // [firmware_ram.start..firmware_start] RWX
        // [firmware_start..firmware_rodata_start] R
        // [firmware_rodata_start..firmware_rodata_end] NONE
        // [firmware_rodata_end..firmware_end] NONE
        // [firmware_end..firmware_ram.end] RWX
        // [firmware_ram.end..INF] RWX
        use riscv::register::*;

        asm!("la {}, sbi_start", out(reg) firmware_start, options(nomem));
        let firmware_end = locate_firmware_image()
            .expect("firmware bounds are available after topology initialization")
            .end()
            .as_usize();
        asm!(
            "la {}, sbi_rodata_start",
            out(reg) rodata_start,
            options(nomem)
        );
        asm!(
            "la {}, sbi_rodata_end",
            out(reg) rodata_end,
            options(nomem)
        );

        assert_eq!(firmware_ram.start & 0x3, 0);
        assert_eq!(firmware_ram.end & 0x3, 0);
        assert_eq!(firmware_start & 0x3, 0);
        assert_eq!(firmware_end & 0x3, 0);
        assert_eq!(rodata_start & 0x3, 0);
        assert_eq!(rodata_end & 0x3, 0);

        // This layout assumes QEMU's CLINT, APLIC, and IMSIC address order.
        // Keep those controllers inaccessible when firmware retains IMSIC IPIs.
        if crate::services::board_info().model == crate::driver::irqchip::aplic::QEMU_VIRT_MODEL
            && matches!(
                crate::services::ipi_delivery(),
                Some(IpiDelivery::Imsic { .. })
            )
            && let Some(imsic) = crate::services::board_info().devices.interrupts.imsic()
        {
            const QEMU_VIRT_CLINT_BASE: usize = 0x0200_0000;
            const QEMU_VIRT_CLINT_SIZE: usize = 0x1_0000;

            let clint_start = crate::services::board_info()
                .devices
                .interrupts
                .clint()
                .map(|description| description.resource().start().as_usize())
                .unwrap_or(QEMU_VIRT_CLINT_BASE);
            let clint_end = clint_start + QEMU_VIRT_CLINT_SIZE;
            let aplic_registers = crate::services::board_info()
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
                .map(|range| range.end().as_usize())
                .max()
                .expect("BUG: every enabled hart requires an IMSIC file");

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
            pmpaddr8::write(firmware_start >> 2);
            set_pmp_config(9, Range::TOR, Permission::R, false);
            pmpaddr9::write(rodata_start >> 2);
            set_pmp_config(10, Range::TOR, Permission::NONE, false);
            pmpaddr10::write(rodata_end >> 2);
            set_pmp_config(11, Range::TOR, Permission::NONE, false);
            pmpaddr11::write(firmware_end >> 2);
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
        pmpaddr2::write(firmware_start >> 2);
        set_pmp_config(3, Range::TOR, Permission::R, false);
        pmpaddr3::write(rodata_start >> 2);
        set_pmp_config(4, Range::TOR, Permission::NONE, false);
        pmpaddr4::write(rodata_end >> 2);
        set_pmp_config(5, Range::TOR, Permission::NONE, false);
        pmpaddr5::write(firmware_end >> 2);
        set_pmp_config(6, Range::TOR, Permission::RWX, false);
        pmpaddr6::write(firmware_ram.end >> 2);
        if let Some(alias) = crate::services::board_info()
            .memory
            .noncacheable_alias_offset
        {
            assert!(alias.is_power_of_two() && alias >= firmware_ram.end as u64);
            let start = (firmware_start as u64)
                .checked_add(alias)
                .expect("BUG: noncacheable alias start overflowed");
            let end = (firmware_end as u64)
                .checked_add(alias)
                .expect("BUG: noncacheable alias end overflowed");
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

/// A PMP permission formatter.
#[repr(transparent)]
struct PermissionWrapper(pub Permission);

/// A PMP range formatter.
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
pub(crate) fn log_pmp_cfg(_firmware_ram: &Range<usize>) {
    use riscv::register::*;
    let pmp_config_fn = |index: usize| {
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

    let get_pmp_range_fn =
        |index: usize| -> RangeWrapper { RangeWrapper(pmp_config_fn(index).range) };
    let get_pmp_permission_fn =
        |index: usize| -> PermissionWrapper { PermissionWrapper(pmp_config_fn(index).permission) };
    info!("PMP Configuration");

    info!(
        "{:<5} {:<10} {:<15} {:<30}",
        "PMP", "Range", "Permission", "Address"
    );

    let log_entry_fn = |index, address: usize| {
        info!(
            "{:<5} {:<10} {:<15} 0x{:016x}",
            index,
            get_pmp_range_fn(index),
            get_pmp_permission_fn(index),
            (address as u64) << 2,
        );
    };
    seq_macro::seq!(N in 0..8 {
        log_entry_fn(N, pastey::paste! { [<pmpaddr ~N>]::read() });
    });
    if crate::services::board_info()
        .memory
        .noncacheable_alias_offset
        .is_some()
    {
        seq_macro::seq!(N in 8..10 {
            log_entry_fn(N, pastey::paste! { [<pmpaddr ~N>]::read() });
        });
    }
}
