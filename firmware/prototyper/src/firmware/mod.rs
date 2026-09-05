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

use crate::riscv::current_hartid;

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
        let hart_id = current_hartid();
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
                current_hartid() == hart_id
            }
        }
        // Without a readable DynamicInfo, race to elect a single boot hart.
        None => claim_boot_hart(),
    }
}

use alloc::{format, vec};
use core::arch::asm;
use core::ops::Range;

use crate::sbi::hart_context::NextStage;

use serde::Serialize;

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

/// Patches the DTB for the next stage: reserves the firmware image and
/// hides firmware-retained M-level interrupt controllers. Returns the
/// patched DTB address.
pub(crate) fn patch_device_tree(
    device_tree_address: usize,
    board: &crate::platform::BoardInfo,
    firmware_image: runtime::memory::PhysAddrRange,
    uses_imsic: bool,
) -> runtime::Result<usize> {
    use serde_device_tree::buildin::Node;
    use serde_device_tree::ser::serializer::ValueType;
    use serde_device_tree::{Dtb, DtbPtr};
    let dtb_pointer =
        DtbPtr::from_raw(device_tree_address as *mut _).map_err(|_| runtime::Error::InvalidArgs)?;
    let dtb = Dtb::from(dtb_pointer);

    let firmware_start = firmware_image.start().as_usize();
    let firmware_size = firmware_image.size();

    let dtb = dtb.share();
    let root: Node =
        serde_device_tree::from_raw_mut(&dtb).map_err(|_| runtime::Error::InvalidArgs)?;
    let tree: Node = root.deserialize();

    #[derive(Serialize)]
    struct ReservedMemory {
        #[serde(rename = "#address-cells")]
        address_cells: u32,
        #[serde(rename = "#size-cells")]
        size_cells: u32,
        ranges: (),
    }
    #[derive(Serialize)]
    struct ReservedRegion {
        reg: [u32; 4],
        #[serde(rename = "no-map")]
        no_map: (),
    }
    let reserved_memory = ReservedMemory {
        address_cells: 2,
        size_cells: 2,
        ranges: (),
    };
    let [address_high, address_low] = fdt_u64_cells(firmware_start);
    let [size_high, size_low] = fdt_u64_cells(firmware_size);
    let firmware_reservation = ReservedRegion {
        reg: [address_high, address_low, size_high, size_low],
        no_map: (),
    };
    let reserved_memory_patch = serde_device_tree::ser::patch::Patch::new(
        "/reserved-memory",
        &reserved_memory as _,
        ValueType::Node,
    );
    let firmware_node_path = format!("/reserved-memory/mmode_resv1@{firmware_start:x}");
    let firmware_patch = serde_device_tree::ser::patch::Patch::new(
        &firmware_node_path,
        &firmware_reservation as _,
        ValueType::Node,
    );
    let patches = alloc::vec![reserved_memory_patch, firmware_patch];
    // Skip the parent-node patch when `/reserved-memory` already exists.
    let first_patch = if tree.find("/reserved-memory").is_some() {
        1
    } else {
        0
    };
    let patches = &patches[first_patch..];

    let patched_length = serde_device_tree::ser::probe_dtb_length(&tree, patches)
        .map_err(|_| runtime::Error::InvalidArgs)?;

    // Allocate as u64s so the DTB buffer stays 8-byte aligned.
    let patched_dtb_buffer = vec![0u64; patched_length.div_ceil(8)];
    // Intentionally leak the buffer: the returned DTB pointer must remain
    // valid for the firmware's lifetime.
    let patched_dtb_buffer = patched_dtb_buffer.leak();
    // SAFETY: `patched_dtb_buffer` is a leaked, 8-byte-aligned buffer of at
    // least `patched_length` bytes.
    let mut patched_dtb_buffer_u8: &'static mut [u8] = unsafe {
        core::slice::from_raw_parts_mut(patched_dtb_buffer.as_ptr() as *mut u8, patched_length)
    };
    serde_device_tree::ser::to_dtb(&tree, patches, &mut patched_dtb_buffer_u8)
        .map_err(|_| runtime::Error::InvalidArgs)?;

    // Hide machine-level interrupt controllers only when firmware retained
    // them by selecting the IMSIC device.
    if uses_imsic {
        // SAFETY: same leaked buffer and length as above; the slice is
        // recreated for the in-place node patching below.
        let dtb_buf = unsafe {
            core::slice::from_raw_parts_mut(patched_dtb_buffer.as_ptr() as *mut u8, patched_length)
        };
        fdt_nop_m_level_imsic(dtb_buf);
        if let Some((clint, _)) = board.clint.as_ref() {
            let clint_name = format!("clint@{:x}", clint.start().as_usize());
            if fdt_nop_node_by_name(dtb_buf, &clint_name) {
                info!("AIA: NOP'd M-level CLINT node '{}' in DTB", clint_name);
            }
        }
        fdt_nop_m_level_aplic(dtb_buf);
    }

    info!(
        "The patched dtb is located at 0x{:x} with length 0x{:x}.",
        patched_dtb_buffer.as_ptr() as usize,
        patched_length
    );
    Ok(patched_dtb_buffer.as_ptr() as usize)
}

fn fdt_u64_cells(value: usize) -> [u32; 2] {
    let value = value as u64;
    [(value >> u32::BITS) as u32, value as u32]
}

// TODO: Move these raw FDT structure block patch helpers to serde-device-tree.
const FDT_BEGIN_NODE: u32 = 0x01;
const FDT_END_NODE: u32 = 0x02;
const FDT_PROP: u32 = 0x03;
const FDT_NOP: u32 = 0x04;
const MACHINE_EXTERNAL_INTERRUPT_ID: u32 = 11;

fn fdt_read_u32(buffer: &[u8], offset: usize) -> u32 {
    u32::from_be_bytes([
        buffer[offset],
        buffer[offset + 1],
        buffer[offset + 2],
        buffer[offset + 3],
    ])
}

fn fdt_write_u32(buffer: &mut [u8], offset: usize, value: u32) {
    let bytes = value.to_be_bytes();
    buffer[offset..offset + 4].copy_from_slice(&bytes);
}

fn fdt_nop_node_by_name(dtb: &mut [u8], target_name: &str) -> bool {
    let structure_offset = fdt_read_u32(dtb, 8) as usize;
    let structure_size = fdt_read_u32(dtb, 36) as usize;
    let structure_end = structure_offset + structure_size;
    let mut offset = structure_offset;

    while offset + 4 <= structure_end {
        let token = fdt_read_u32(dtb, offset);
        match token {
            FDT_BEGIN_NODE => {
                let name_start = offset + 4;
                let name = core::ffi::CStr::from_bytes_until_nul(&dtb[name_start..])
                    .map(|name| name.to_str().unwrap_or(""))
                    .unwrap_or("");
                if name == target_name {
                    let node_start = offset;
                    let mut depth = 1u32;
                    offset += 4 + ((name.len() + 4) & !3);
                    while depth > 0 && offset + 4 <= structure_end {
                        let token = fdt_read_u32(dtb, offset);
                        if token == FDT_BEGIN_NODE {
                            depth += 1;
                            let nested_name =
                                core::ffi::CStr::from_bytes_until_nul(&dtb[offset + 4..])
                                    .map(|name| name.to_str().unwrap_or(""))
                                    .unwrap_or("");
                            offset += 4 + ((nested_name.len() + 4) & !3);
                        } else if token == FDT_END_NODE {
                            depth -= 1;
                            offset += 4;
                        } else if token == FDT_PROP {
                            let property_size = fdt_read_u32(dtb, offset + 4) as usize;
                            offset += 12 + ((property_size + 3) & !3);
                        } else if token == FDT_NOP {
                            offset += 4;
                        } else {
                            break;
                        }
                    }
                    for word_offset in (node_start..offset).step_by(4) {
                        fdt_write_u32(dtb, word_offset, FDT_NOP);
                    }
                    return true;
                }
                offset += 4 + ((name.len() + 4) & !3);
            }
            FDT_END_NODE => {
                offset += 4;
            }
            FDT_PROP => {
                let property_size = fdt_read_u32(dtb, offset + 4) as usize;
                offset += 12 + ((property_size + 3) & !3);
            }
            FDT_NOP => {
                offset += 4;
            }
            _ => break,
        }
    }
    false
}

fn fdt_interrupts_extended_has_irq(property_value: &[u8], irq: u32) -> bool {
    let mut chunks = property_value.chunks_exact(8);
    let mut found = false;
    for interrupt in chunks.by_ref() {
        let interrupt_id =
            u32::from_be_bytes([interrupt[4], interrupt[5], interrupt[6], interrupt[7]]);
        if interrupt_id == irq {
            found = true;
        }
    }
    found && chunks.remainder().is_empty()
}

fn fdt_compatible_matches(property_value: &[u8], compatibles: &[&str]) -> bool {
    property_value.split(|byte| *byte == 0).any(|candidate| {
        compatibles
            .iter()
            .any(|compatible| candidate == compatible.as_bytes())
    })
}

fn fdt_nop_m_level_imsic(dtb: &mut [u8]) {
    let structure_offset = fdt_read_u32(dtb, 8) as usize;
    let structure_size = fdt_read_u32(dtb, 36) as usize;
    let strings_offset = fdt_read_u32(dtb, 12) as usize;
    let structure_end = structure_offset + structure_size;
    let mut offset = structure_offset;

    while offset + 4 <= structure_end {
        let token = fdt_read_u32(dtb, offset);
        match token {
            FDT_BEGIN_NODE => {
                let name_start = offset + 4;
                let name = core::ffi::CStr::from_bytes_until_nul(&dtb[name_start..])
                    .map(|name| name.to_str().unwrap_or(""))
                    .unwrap_or("");
                let node_start = offset;
                let name_len = name.len();
                offset += 4 + ((name_len + 4) & !3);

                let mut is_imsic = false;
                let mut has_machine_external_interrupt = false;
                let mut scan_offset = offset;
                let mut depth = 1u32;

                while depth > 0 && scan_offset + 4 <= structure_end {
                    let token = fdt_read_u32(dtb, scan_offset);
                    if token == FDT_BEGIN_NODE {
                        depth += 1;
                        let nested_name =
                            core::ffi::CStr::from_bytes_until_nul(&dtb[scan_offset + 4..])
                                .map(|name| name.to_str().unwrap_or(""))
                                .unwrap_or("");
                        scan_offset += 4 + ((nested_name.len() + 4) & !3);
                    } else if token == FDT_END_NODE {
                        depth -= 1;
                        scan_offset += 4;
                    } else if token == FDT_PROP {
                        let property_size = fdt_read_u32(dtb, scan_offset + 4) as usize;
                        let name_offset = fdt_read_u32(dtb, scan_offset + 8) as usize;
                        let property_name = core::ffi::CStr::from_bytes_until_nul(
                            &dtb[strings_offset + name_offset..],
                        )
                        .map(|name| name.to_str().unwrap_or(""))
                        .unwrap_or("");
                        let property_value =
                            &dtb[scan_offset + 12..scan_offset + 12 + property_size];
                        if depth == 1
                            && property_name == "compatible"
                            && property_size > 0
                            && fdt_compatible_matches(
                                property_value,
                                &crate::driver::IMSIC_COMPATIBLES,
                            )
                        {
                            is_imsic = true;
                        }
                        if depth == 1
                            && property_name == "interrupts-extended"
                            && fdt_interrupts_extended_has_irq(
                                property_value,
                                MACHINE_EXTERNAL_INTERRUPT_ID,
                            )
                        {
                            has_machine_external_interrupt = true;
                        }
                        scan_offset += 12 + ((property_size + 3) & !3);
                    } else if token == FDT_NOP {
                        scan_offset += 4;
                    } else {
                        break;
                    }
                }

                if is_imsic && has_machine_external_interrupt {
                    let node_name = alloc::string::String::from(name);
                    for word_offset in (node_start..scan_offset).step_by(4) {
                        fdt_write_u32(dtb, word_offset, FDT_NOP);
                    }
                    info!("AIA: NOP'd M-level IMSIC node '{}' in DTB", node_name);
                }
            }
            FDT_END_NODE => {
                offset += 4;
            }
            FDT_PROP => {
                let property_size = fdt_read_u32(dtb, offset + 4) as usize;
                offset += 12 + ((property_size + 3) & !3);
            }
            FDT_NOP => {
                offset += 4;
            }
            _ => break,
        }
    }
}

fn fdt_nop_m_level_aplic(dtb: &mut [u8]) {
    let structure_offset = fdt_read_u32(dtb, 8) as usize;
    let structure_size = fdt_read_u32(dtb, 36) as usize;
    let strings_offset = fdt_read_u32(dtb, 12) as usize;
    let structure_end = structure_offset + structure_size;
    let mut offset = structure_offset;

    while offset + 4 <= structure_end {
        let token = fdt_read_u32(dtb, offset);
        match token {
            FDT_BEGIN_NODE => {
                let name_start = offset + 4;
                let name = core::ffi::CStr::from_bytes_until_nul(&dtb[name_start..])
                    .map(|name| name.to_str().unwrap_or(""))
                    .unwrap_or("");
                let node_start = offset;
                let name_len = name.len();
                offset += 4 + ((name_len + 4) & !3);

                let mut is_aplic = false;
                let mut has_delegation = false;
                let mut scan_offset = offset;
                let mut depth = 1u32;

                while depth > 0 && scan_offset + 4 <= structure_end {
                    let token = fdt_read_u32(dtb, scan_offset);
                    if token == FDT_BEGIN_NODE {
                        depth += 1;
                        let nested_name =
                            core::ffi::CStr::from_bytes_until_nul(&dtb[scan_offset + 4..])
                                .map(|name| name.to_str().unwrap_or(""))
                                .unwrap_or("");
                        scan_offset += 4 + ((nested_name.len() + 4) & !3);
                    } else if token == FDT_END_NODE {
                        depth -= 1;
                        scan_offset += 4;
                    } else if token == FDT_PROP {
                        let property_size = fdt_read_u32(dtb, scan_offset + 4) as usize;
                        let name_offset = fdt_read_u32(dtb, scan_offset + 8) as usize;
                        let property_name = core::ffi::CStr::from_bytes_until_nul(
                            &dtb[strings_offset + name_offset..],
                        )
                        .map(|name| name.to_str().unwrap_or(""))
                        .unwrap_or("");
                        if depth == 1 && property_name == "compatible" && property_size > 0 {
                            let property_value =
                                &dtb[scan_offset + 12..scan_offset + 12 + property_size];
                            if property_value
                                .windows(11)
                                .any(|window| window == b"riscv,aplic")
                            {
                                is_aplic = true;
                            }
                        }
                        if depth == 1
                            && (property_name == "riscv,delegate"
                                || property_name == "riscv,delegation")
                        {
                            has_delegation = true;
                        }
                        scan_offset += 12 + ((property_size + 3) & !3);
                    } else if token == FDT_NOP {
                        scan_offset += 4;
                    } else {
                        break;
                    }
                }

                if is_aplic && has_delegation {
                    let node_name = alloc::string::String::from(name);
                    for word_offset in (node_start..scan_offset).step_by(4) {
                        fdt_write_u32(dtb, word_offset, FDT_NOP);
                    }
                    info!("AIA: NOP'd M-level APLIC node '{}' in DTB", node_name);
                }
            }
            FDT_END_NODE => {
                offset += 4;
            }
            FDT_PROP => {
                let property_size = fdt_read_u32(dtb, offset + 4) as usize;
                offset += 12 + ((property_size + 3) & !3);
            }
            FDT_NOP => {
                offset += 4;
            }
            _ => break,
        }
    }
}

static mut FIRMWARE_START_ADDRESS: usize = 0;
static mut FIRMWARE_END_ADDRESS: usize = 0;
static mut FIRMWARE_RODATA_START_ADDRESS: usize = 0;
static mut FIRMWARE_RODATA_END_ADDRESS: usize = 0;

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
        if crate::sbi::ipi::uses_imsic()
            && crate::platform::board_info().is_qemu_virt()
            && let Some(imsic) = crate::platform::board_info().imsic.as_ref()
        {
            const QEMU_VIRT_CLINT_BASE: usize = 0x0200_0000;
            const QEMU_VIRT_CLINT_SIZE: usize = 0x1_0000;

            let clint_start = crate::platform::board_info()
                .clint
                .as_ref()
                .map(|(registers, _)| registers.start().as_usize())
                .unwrap_or(QEMU_VIRT_CLINT_BASE);
            let clint_end = clint_start + QEMU_VIRT_CLINT_SIZE;
            let aplic_registers = crate::platform::board_info()
                .machine_aplic
                .expect("BUG: QEMU AIA setup requires a machine APLIC");
            let aplic_start = aplic_registers.start().as_usize();
            let aplic_end = aplic_registers.end().as_usize();
            let machine_imsic_start = imsic.layout.machine_base.as_usize();
            let machine_imsic_end = imsic
                .hart_files
                .iter()
                .flatten()
                .map(|range| range.end().as_usize())
                .max()
                .unwrap_or(machine_imsic_start + 0x1000);

            pmpcfg0::set_pmp(0, Range::OFF, Permission::NONE, false);
            pmpaddr0::write(0);
            pmpcfg0::set_pmp(1, Range::TOR, Permission::RWX, false);
            pmpaddr1::write(clint_start >> 2);
            pmpcfg0::set_pmp(2, Range::TOR, Permission::NONE, false);
            pmpaddr2::write(clint_end >> 2);
            pmpcfg0::set_pmp(3, Range::TOR, Permission::RWX, false);
            pmpaddr3::write(aplic_start >> 2);
            pmpcfg0::set_pmp(4, Range::TOR, Permission::NONE, false);
            pmpaddr4::write(aplic_end >> 2);
            pmpcfg0::set_pmp(5, Range::TOR, Permission::RWX, false);
            pmpaddr5::write(machine_imsic_start >> 2);
            pmpcfg0::set_pmp(6, Range::TOR, Permission::NONE, false);
            pmpaddr6::write(machine_imsic_end >> 2);
            pmpcfg0::set_pmp(7, Range::TOR, Permission::RWX, false);
            pmpaddr7::write(firmware_ram.start >> 2);
            pmpcfg2::set_pmp(0, Range::TOR, Permission::RWX, false);
            pmpaddr8::write(FIRMWARE_START_ADDRESS >> 2);
            pmpcfg2::set_pmp(1, Range::TOR, Permission::R, false);
            pmpaddr9::write(FIRMWARE_RODATA_START_ADDRESS >> 2);
            pmpcfg2::set_pmp(2, Range::TOR, Permission::NONE, false);
            pmpaddr10::write(FIRMWARE_RODATA_END_ADDRESS >> 2);
            pmpcfg2::set_pmp(3, Range::TOR, Permission::RW, false);
            pmpaddr11::write(FIRMWARE_END_ADDRESS >> 2);
            pmpcfg2::set_pmp(4, Range::TOR, Permission::RWX, false);
            pmpaddr12::write(firmware_ram.end >> 2);
            pmpcfg2::set_pmp(5, Range::TOR, Permission::RWX, false);
            pmpaddr13::write(usize::MAX >> 2);
            return;
        }

        pmpcfg0::set_pmp(0, Range::OFF, Permission::NONE, false);
        pmpaddr0::write(0);
        pmpcfg0::set_pmp(1, Range::TOR, Permission::RWX, false);
        pmpaddr1::write(firmware_ram.start >> 2);
        pmpcfg0::set_pmp(2, Range::TOR, Permission::RWX, false);
        pmpaddr2::write(FIRMWARE_START_ADDRESS >> 2);
        pmpcfg0::set_pmp(3, Range::TOR, Permission::R, false);
        pmpaddr3::write(FIRMWARE_RODATA_START_ADDRESS >> 2);
        pmpcfg0::set_pmp(4, Range::TOR, Permission::NONE, false);
        pmpaddr4::write(FIRMWARE_RODATA_END_ADDRESS >> 2);
        pmpcfg0::set_pmp(5, Range::TOR, Permission::RW, false); // FIXME: should be `R`; `RW` temporarily allows S-mode DTB modification
        pmpaddr5::write(FIRMWARE_END_ADDRESS >> 2);
        pmpcfg0::set_pmp(6, Range::TOR, Permission::RWX, false);
        pmpaddr6::write(firmware_ram.end >> 2);
        pmpcfg0::set_pmp(7, Range::TOR, Permission::RWX, false);
        pmpaddr7::write(usize::MAX >> 2);
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
    let pmp_config = pmpcfg0::read();

    let get_pmp_range =
        |index: usize| -> RangeWrapper { RangeWrapper(pmp_config.into_config(index).range) };
    let get_pmp_permission = |index: usize| -> PermissionWrapper {
        PermissionWrapper(pmp_config.into_config(index).permission)
    };
    info!("PMP Configuration");

    info!(
        "{:<5} {:<10} {:<15} {:<30}",
        "PMP", "Range", "Permission", "Address"
    );

    seq_macro::seq!(N in 0..8 {
        info!(
            "{:<5} {:<10} {:<15} 0x{:016x}",
            N,
            get_pmp_range(N),
            get_pmp_permission(N),
            pastey::paste! { [<pmpaddr ~N>]::read() } << 2,
        );
    });
}

#[cfg(all(feature = "fdt", not(feature = "payload")))]
include!(concat!(env!("OUT_DIR"), "/generated_alignment.rs"));
#[cfg(all(feature = "fdt", not(feature = "payload")))]
include!(concat!(env!("OUT_DIR"), "/generated_fdt.rs"));
