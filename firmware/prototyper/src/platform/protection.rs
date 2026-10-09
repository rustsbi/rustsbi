//! Firmware memory-protection policy.

use core::ops::Range;

/// Installs the firmware PMP plan and returns the number of installed entries.
pub(crate) fn install(firmware_ram: &Range<usize>) -> usize {
    use runtime::pmp::{Entry, Permission, Pmp, Region};

    let pmp = Pmp::current().expect("BUG: PMP setup is outside the published hart topology");

    let layout = runtime::memory::firmware_image_layout()
        .expect("BUG: linker supplied an invalid firmware image layout");
    let image = layout.image_range();
    let rodata = layout.read_only_range();
    let (firmware_start, firmware_end, rodata_start, rodata_end) = (
        image.start().as_usize() as u64,
        image.end().as_usize() as u64,
        rodata.start().as_usize() as u64,
        rodata.end().as_usize() as u64,
    );

    let off_fn = |address| Entry {
        region: Region::Off(address),
        permission: Permission::None,
        locked: false,
    };
    let tor_fn = |address, permission| Entry {
        region: Region::Tor(address),
        permission,
        locked: false,
    };
    let address_space_end = Entry {
        region: Region::NativeAddressSpaceEnd,
        permission: Permission::ReadWriteExecute,
        locked: false,
    };
    let firmware_regions = [
        tor_fn(firmware_ram.start as u64, Permission::ReadWriteExecute),
        tor_fn(firmware_start, Permission::ReadWriteExecute),
        tor_fn(rodata_start, Permission::Read),
        tor_fn(rodata_end, Permission::None),
        // FIXME: should be read-only; S-mode currently modifies the DTB here.
        tor_fn(firmware_end, Permission::ReadWrite),
        tor_fn(firmware_ram.end as u64, Permission::ReadWriteExecute),
    ];

    let board = crate::platform::board_info();
    let mut protected_regions = alloc::vec::Vec::new();
    if crate::driver::ipi::uses_imsic()
        && board.is_qemu_virt()
        && let Some(imsic) = board.devices.interrupts.imsic()
    {
        const QEMU_VIRT_CLINT_BASE: usize = 0x0200_0000;
        const QEMU_VIRT_CLINT_SIZE: usize = 0x1_0000;
        let clint_start = board
            .devices
            .interrupts
            .clint()
            .map(|description| description.resource().registers.start())
            .unwrap_or(runtime::memory::PhysAddr::new(QEMU_VIRT_CLINT_BASE));
        protected_regions.push(
            runtime::memory::PhysAddrRange::from_start_len(clint_start, QEMU_VIRT_CLINT_SIZE)
                .expect("BUG: QEMU CLINT range is invalid"),
        );
        let aplic = board
            .devices
            .interrupts
            .machine_aplic()
            .expect("BUG: QEMU AIA setup requires a machine APLIC")
            .resource();
        protected_regions.push(
            runtime::memory::PhysAddrRange::new(aplic.start(), aplic.end())
                .expect("BUG: QEMU APLIC range is invalid"),
        );
        let imsic = imsic.resource();
        let machine_imsic_end = imsic
            .hart_files
            .iter()
            .map(|range| range.end())
            .max_by_key(|address| address.as_usize())
            .expect("BUG: every enabled hart requires an IMSIC file");
        protected_regions.push(
            runtime::memory::PhysAddrRange::new(imsic.layout.machine_base, machine_imsic_end)
                .expect("BUG: QEMU IMSIC range is invalid"),
        );
    }
    let protected_regions = protected_regions.as_slice();
    // Interrupt policy supplies ordered, non-overlapping machine windows.
    // Two TOR entries per window deny it while preserving the gaps.
    let mut plan = [off_fn(0); 14];
    let mut count = 1;
    for range in protected_regions {
        plan[count] = tor_fn(
            range.start().as_usize() as u64,
            Permission::ReadWriteExecute,
        );
        plan[count + 1] = tor_fn(range.end().as_usize() as u64, Permission::None);
        count += 2;
    }
    plan[count..count + firmware_regions.len()].copy_from_slice(&firmware_regions);
    count += firmware_regions.len();
    if protected_regions.is_empty()
        && let Some(alias) = board.memory.noncacheable_alias_offset
    {
        assert!(alias.is_power_of_two() && alias >= firmware_ram.end as u64);
        let start = firmware_start
            .checked_add(alias)
            .expect("BUG: firmware alias start overflowed");
        let end = firmware_end
            .checked_add(alias)
            .expect("BUG: firmware alias end overflowed");
        // Deny the firmware alias before permitting the wider address space.
        plan[count] = off_fn(start);
        plan[count + 1] = tor_fn(end, Permission::None);
        plan[count + 2] = Entry {
            region: Region::AllMemory,
            permission: Permission::ReadWriteExecute,
            locked: false,
        };
        count += 3;
    } else {
        plan[count] = address_space_end;
        count += 1;
    }
    pmp.install(&plan[..count])
        .expect("failed to install firmware protection plan");
    count
}

/// Logs the active PMP configuration.
pub(crate) fn log(count: usize) {
    let configuration = runtime::pmp::Pmp::current()
        .expect("BUG: PMP logging is outside the published hart topology")
        .snapshot(count)
        .expect("failed to read firmware protection configuration");

    info!("PMP Configuration");
    info!(
        "{:<5} {:<10} {:<15} {:<30}",
        "PMP", "Range", "Permission", "Address"
    );
    for (index, entry) in configuration.iter().enumerate().take(count) {
        info!(
            "{:<5} {:<10} {:<15} 0x{:016x}",
            index,
            entry.mode(),
            entry.permission(),
            entry.address_field(),
        );
    }
}
