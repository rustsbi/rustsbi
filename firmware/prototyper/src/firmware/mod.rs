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

use core::ops::Range;

use runtime::boot::NextStage;

/// Boot information decoded from the previous-stage register envelope (a1/a2).
pub(crate) struct BootInfo {
    device_tree_address: usize,
    is_boot_hart: bool,
    platform_description: Option<runtime::PlatformDescription>,
    /// The `a2` `DynamicInfo` address, kept for the deferred `next_stage()`
    /// decode.
    dynamic_info_address: usize,
}

impl BootInfo {
    /// Decodes a boot candidate's handoff under the entry bootstrap lock.
    ///
    /// Without a designated hart, the first enabled candidate leads boot.
    pub(crate) fn decode(
        device_tree: runtime::DeviceTreeHandoff,
        dynamic_info_address: usize,
    ) -> runtime::Result<Self> {
        let current_hart = runtime::csr::mhartid();
        let designated_hart = designated_boot_hart(dynamic_info_address);
        // Only the designated boot hart's FDT belongs to platform discovery.
        if designated_hart.is_some_and(|hart| hart != current_hart) {
            return Ok(Self::secondary(device_tree, dynamic_info_address));
        }
        let device_tree_address = resolve_device_tree_address(device_tree.address().as_usize());
        let platform = device_tree.claim(runtime::memory::PhysAddr::new(device_tree_address))?;
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
            device_tree_address,
            is_boot_hart,
            platform_description: is_boot_hart.then_some(platform),
            dynamic_info_address,
        })
    }

    /// Constructs a secondary handoff without reading the boot hart's FDT.
    pub(crate) fn secondary(
        device_tree: runtime::DeviceTreeHandoff,
        dynamic_info_address: usize,
    ) -> Self {
        Self {
            device_tree_address: resolve_device_tree_address(device_tree.address().as_usize()),
            is_boot_hart: false,
            platform_description: None,
            dynamic_info_address,
        }
    }

    pub(crate) fn platform_description(&self) -> &runtime::PlatformDescription {
        self.platform_description
            .as_ref()
            .expect("boot hart owns its platform description")
    }

    /// Returns the dynamic boot information's storage range, if present.
    ///
    /// Jump and payload modes return `None`. Stack placement checks this range
    /// independently of the device tree's storage.
    pub(crate) fn dynamic_info_range(
        &self,
    ) -> runtime::Result<Option<runtime::memory::PhysAddrRange>> {
        #[cfg(not(any(feature = "payload", feature = "jump")))]
        {
            runtime::memory::PhysAddrRange::from_start_len(
                runtime::memory::PhysAddr::new(self.dynamic_info_address),
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
    pub(crate) fn take_platform_description(&mut self) -> Option<runtime::PlatformDescription> {
        self.platform_description.take()
    }

    /// Returns the next-stage handoff; `opaque` carries the unpatched
    /// device tree address. Invalid `DynamicInfo` stops boot.
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
    runtime::boot::embedded_fdt()
        .expect("BUG: fdt firmware has no embedded device tree")
        .start()
        .as_usize()
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

/// Installs the firmware PMP plan and returns the number of installed entries.
pub fn set_pmp(firmware_ram: &Range<usize>) -> usize {
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
pub fn log_pmp_cfg(count: usize) {
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
