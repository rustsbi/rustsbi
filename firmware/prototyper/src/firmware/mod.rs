mod pmp;
pub(crate) use pmp::{log_pmp_cfg, set_pmp};

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

use runtime::{
    DeviceTreeHandoff, Error, PlatformDescription, Result,
    boot::NextStage,
    memory::{PhysAddr, PhysAddrRange},
};

/// Boot information decoded from the previous-stage register envelope (a1/a2).
pub(crate) struct BootInfo {
    /// The unmodified device-tree address received in `a1`.
    device_tree_address: usize,
    is_boot_hart: bool,
    platform_description: Option<PlatformDescription>,
    /// The `a2` `DynamicInfo` address, kept for the deferred `next_stage()`
    /// decode.
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
        let destination = match crate::cfg::FDT_HANDOFF {
            Some((address, capacity)) => PhysAddrRange::from_start_len(
                PhysAddr::new(usize::try_from(address).map_err(|_| Error::Overflow)?),
                usize::try_from(capacity).map_err(|_| Error::Overflow)?,
            )?,
            // SAFETY: without an explicit destination, entry requires an incoming
            // FDT with exclusive writable storage for totalsize + 1024 bytes.
            None => unsafe { device_tree.incoming_destination()? },
        };
        // SAFETY: entry guarantees a readable, stable source (linked with `fdt`)
        // and exclusive writable destination storage until handoff. The latter
        // remains live until the next stage finishes using it. BOOT_LOCK serializes
        // candidate claims until the boot owner is selected.
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
        // SAFETY: Runtime's loader entry contract establishes the target's
        // architectural calling convention and the incoming FDT's lifetime.
        unsafe { NextStage::new(start_address, self.device_tree_address, next_mode) }
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

#[cfg(all(feature = "fdt", not(feature = "payload")))]
include!(concat!(env!("OUT_DIR"), "/generated_alignment.rs"));
#[cfg(all(feature = "fdt", not(feature = "payload")))]
include!(concat!(env!("OUT_DIR"), "/generated_fdt.rs"));
