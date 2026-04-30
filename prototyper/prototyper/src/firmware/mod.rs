cfg_if::cfg_if! {
    if #[cfg(feature = "payload")] {
        pub mod payload;
        pub use payload::{get_boot_info};
    } else if #[cfg(feature = "jump")] {
        pub mod jump;
        pub use jump::{get_boot_info};
    } else {
        pub mod dynamic;
        pub use dynamic::{get_boot_info, read_paddr};
    }
}

use core::fmt;

use riscv::register::{self, Permission};

use crate::riscv::current_hartid;

/// Get work hart, for both steps.
///
/// Init hart can be random choose when DynamicInfo can not be read.
pub fn is_work_hart(_nonstandard_a2: usize, _boot: bool) -> bool {
    use core::sync::atomic::{AtomicUsize, Ordering};
    static WORK_HART: AtomicUsize = AtomicUsize::new(usize::MAX);

    cfg_if::cfg_if! {
        if #[cfg(any(feature = "payload", feature = "jump"))] {
            let info: _ = None;
        }
        else {
            let info = read_paddr(_nonstandard_a2).ok().and_then(|x| Some(x.boot_hart));
        }
    }

    let select_work_hart = || {
        let hart_id = current_hartid();
        match WORK_HART.compare_exchange(usize::MAX, hart_id, Ordering::AcqRel, Ordering::Acquire) {
            Ok(_) => true,
            Err(selected_hart) => selected_hart == hart_id,
        }
    };

    // Determine if this is the boot hart based on hart ID
    match info {
        Some(info) => {
            if info == usize::MAX {
                select_work_hart()
            } else {
                // Otherwise check if current hart matches designated boot hart
                current_hartid() == info
            }
        }
        // If can not load DynamicInfo, just race a boot hart, this error will
        // be occurred after board init.
        None => select_work_hart(),
    }
}

use alloc::{format, vec};
#[allow(unused)]
use core::arch::{asm, naked_asm};
use core::ops::Range;

use crate::fail;

use riscv::register::mstatus;
use serde::Serialize;

pub struct BootInfo {
    pub next_address: usize,
    pub mpp: mstatus::MPP,
}

pub struct BootHart {
    pub fdt_address: usize,
    pub is_boot_hart: bool,
}

#[unsafe(naked)]
#[unsafe(link_section = ".fdt")]
#[rustc_align(16)]
#[cfg(feature = "fdt")]
pub extern "C" fn raw_fdt() {
    naked_asm!(concat!(".incbin \"", env!("PROTOTYPER_FDT_PATH"), "\""),)
}

#[inline]
#[cfg(feature = "fdt")]
fn get_fdt_address() -> usize {
    raw_fdt as usize
}

/// Gets boot hart information based on opaque and nonstandard_a2 parameters.
///
/// Returns a BootHart struct containing FDT address and whether this is the boot hart.
///
/// The boot flow is splitted into two steps, first init all devices,
/// second the really boot stage. When in second step, boot flag should be true.
#[allow(unused_mut, unused_assignments)]
pub fn get_work_hart(opaque: usize, nonstandard_a2: usize, boot: bool) -> BootHart {
    let is_boot_hart = is_work_hart(nonstandard_a2, boot);

    let mut fdt_address = opaque;

    #[cfg(feature = "fdt")]
    {
        fdt_address = get_fdt_address();
    }

    BootHart {
        fdt_address,
        is_boot_hart,
    }
}

pub fn patch_device_tree(device_tree_ptr: usize) -> usize {
    use serde_device_tree::buildin::Node;
    use serde_device_tree::ser::serializer::ValueType;
    use serde_device_tree::{Dtb, DtbPtr};
    let Ok(ptr) = DtbPtr::from_raw(device_tree_ptr as *mut _) else {
        panic!("Can not parse device tree!");
    };
    let dtb = Dtb::from(ptr);

    // Update const
    unsafe {
        asm!("la {}, sbi_start", out(reg) SBI_START_ADDRESS, options(nomem));
        asm!("la {}, sbi_end", out(reg) SBI_END_ADDRESS, options(nomem));
    }
    let sbi_start = unsafe { SBI_START_ADDRESS };
    let sbi_end = unsafe { SBI_END_ADDRESS };

    let dtb = dtb.share();
    let root: Node =
        serde_device_tree::from_raw_mut(&dtb).unwrap_or_else(fail::device_tree_deserialize_root);
    let tree: Node = root.deserialize();

    #[derive(Serialize)]
    struct ReservedMemory {
        #[serde(rename = "#address-cells")]
        pub address_cell: u32,
        #[serde(rename = "#size-cells")]
        pub size_cell: u32,
        pub ranges: (),
    }
    #[derive(Serialize)]
    struct ReservedMemoryItem {
        pub reg: [u32; 4],
        #[serde(rename = "no-map")]
        pub no_map: (),
    }
    // Make patch list and generate reserved-memory node.
    let sbi_length: u32 = (sbi_end - sbi_start) as u32;
    let new_base = ReservedMemory {
        address_cell: 2,
        size_cell: 2,
        ranges: (),
    };
    let new_base_2 = ReservedMemoryItem {
        reg: [(sbi_start >> 32) as u32, sbi_start as u32, 0, sbi_length],
        no_map: (),
    };
    let patch1 = serde_device_tree::ser::patch::Patch::new(
        "/reserved-memory",
        &new_base as _,
        ValueType::Node,
    );
    let path_name = format!("/reserved-memory/mmode_resv1@{:x}", sbi_start);
    let patch2 =
        serde_device_tree::ser::patch::Patch::new(&path_name, &new_base_2 as _, ValueType::Node);
    let patches = alloc::vec![patch1, patch2];
    // Only add `reserved-memory` section when it not exists.
    let start_idx = if tree.find("/reserved-memory").is_some() {
        1
    } else {
        0
    };

    let list = &patches[start_idx..];

    let patched_length = serde_device_tree::ser::probe_dtb_length(&tree, &list).unwrap();

    // We need aligned address here, so we use create u64 vec.
    let patched_dtb_buffer = vec![0u64; patched_length.div_ceil(8)];
    // Intentionally leak the buffer so that the patched DTB remains valid for the lifetime of the firmware.
    // This is required because the returned pointer is used elsewhere and must not be deallocated.
    let patched_dtb_buffer = patched_dtb_buffer.leak();
    let mut patched_dtb_buffer_u8: &'static mut [u8] = unsafe {
        core::slice::from_raw_parts_mut(patched_dtb_buffer.as_ptr() as *mut u8, patched_length)
    };
    serde_device_tree::ser::to_dtb(&tree, &list, &mut patched_dtb_buffer_u8).unwrap();

    // When AIA is active, NOP out M-level IMSIC and APLIC nodes in the
    // DTB so Linux does not try to probe them. This matches OpenSBI's
    // fdt_domain_based_fixup approach.
    if crate::platform::aia::is_aia_active() {
        let dtb_buf = unsafe {
            core::slice::from_raw_parts_mut(patched_dtb_buffer.as_ptr() as *mut u8, patched_length)
        };
        fdt_nop_m_level_imsic(dtb_buf);
        if let Some((clint_base, _)) = unsafe { crate::platform::PLATFORM.info.ipi.as_ref() } {
            let clint_name = format!("clint@{:x}", clint_base);
            if fdt_nop_node_by_name(dtb_buf, &clint_name) {
                info!("AIA: NOP'd M-level CLINT node '{}' in DTB", clint_name);
            }
        }
        // Also NOP the M-level APLIC, whose DT node may use either the
        // generic interrupt-controller name or the APLIC-specific name.
        fdt_nop_m_level_aplic(dtb_buf);
    }

    info!(
        "The patched dtb is located at 0x{:x} with length 0x{:x}.",
        patched_dtb_buffer.as_ptr() as usize,
        patched_length
    );
    patched_dtb_buffer.as_ptr() as usize
}

// TODO: Move these raw FDT structure block patch helpers to serde-device-tree.
const FDT_BEGIN_NODE: u32 = 0x01;
const FDT_END_NODE: u32 = 0x02;
const FDT_PROP: u32 = 0x03;
const FDT_NOP: u32 = 0x04;
const RISCV_MACHINE_EXTERNAL_IRQ: u32 = 11;

fn fdt_read_u32(buf: &[u8], off: usize) -> u32 {
    u32::from_be_bytes([buf[off], buf[off + 1], buf[off + 2], buf[off + 3]])
}

fn fdt_write_u32(buf: &mut [u8], off: usize, val: u32) {
    let bytes = val.to_be_bytes();
    buf[off..off + 4].copy_from_slice(&bytes);
}

fn fdt_nop_node_by_name(dtb: &mut [u8], target_name: &str) -> bool {
    let struct_off = fdt_read_u32(dtb, 8) as usize;
    let struct_size = fdt_read_u32(dtb, 36) as usize;
    let end = struct_off + struct_size;
    let mut off = struct_off;

    while off + 4 <= end {
        let token = fdt_read_u32(dtb, off);
        match token {
            t if t == FDT_BEGIN_NODE => {
                let name_start = off + 4;
                let name = core::ffi::CStr::from_bytes_until_nul(&dtb[name_start..])
                    .map(|s| s.to_str().unwrap_or(""))
                    .unwrap_or("");
                if name == target_name {
                    let node_start = off;
                    let mut depth = 1u32;
                    off += 4 + ((name.len() + 4) & !3);
                    while depth > 0 && off + 4 <= end {
                        let t = fdt_read_u32(dtb, off);
                        if t == FDT_BEGIN_NODE {
                            depth += 1;
                            let n = core::ffi::CStr::from_bytes_until_nul(&dtb[off + 4..])
                                .map(|s| s.to_str().unwrap_or(""))
                                .unwrap_or("");
                            off += 4 + ((n.len() + 4) & !3);
                        } else if t == FDT_END_NODE {
                            depth -= 1;
                            off += 4;
                        } else if t == FDT_PROP {
                            let prop_len = fdt_read_u32(dtb, off + 4) as usize;
                            off += 12 + ((prop_len + 3) & !3);
                        } else if t == FDT_NOP {
                            off += 4;
                        } else {
                            break;
                        }
                    }
                    for i in (node_start..off).step_by(4) {
                        fdt_write_u32(dtb, i, FDT_NOP);
                    }
                    return true;
                }
                off += 4 + ((name.len() + 4) & !3);
            }
            t if t == FDT_END_NODE => {
                off += 4;
            }
            t if t == FDT_PROP => {
                let prop_len = fdt_read_u32(dtb, off + 4) as usize;
                off += 12 + ((prop_len + 3) & !3);
            }
            t if t == FDT_NOP => {
                off += 4;
            }
            _ => break,
        }
    }
    false
}

fn fdt_interrupts_extended_has_irq(data: &[u8], irq: u32) -> bool {
    let mut chunks = data.chunks_exact(8);
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

fn fdt_nop_m_level_imsic(dtb: &mut [u8]) {
    let struct_off = fdt_read_u32(dtb, 8) as usize;
    let struct_size = fdt_read_u32(dtb, 36) as usize;
    let strings_off = fdt_read_u32(dtb, 12) as usize;
    let end = struct_off + struct_size;
    let mut off = struct_off;

    while off + 4 <= end {
        let token = fdt_read_u32(dtb, off);
        match token {
            t if t == FDT_BEGIN_NODE => {
                let name_start = off + 4;
                let name = core::ffi::CStr::from_bytes_until_nul(&dtb[name_start..])
                    .map(|s| s.to_str().unwrap_or(""))
                    .unwrap_or("");
                let node_start = off;
                let name_len = name.len();
                off += 4 + ((name_len + 4) & !3);

                let mut is_imsic = false;
                let mut has_m_level_irq = false;
                let mut scan = off;
                let mut depth = 1u32;

                while depth > 0 && scan + 4 <= end {
                    let t = fdt_read_u32(dtb, scan);
                    if t == FDT_BEGIN_NODE {
                        depth += 1;
                        let n = core::ffi::CStr::from_bytes_until_nul(&dtb[scan + 4..])
                            .map(|s| s.to_str().unwrap_or(""))
                            .unwrap_or("");
                        scan += 4 + ((n.len() + 4) & !3);
                    } else if t == FDT_END_NODE {
                        depth -= 1;
                        scan += 4;
                    } else if t == FDT_PROP {
                        let prop_len = fdt_read_u32(dtb, scan + 4) as usize;
                        let name_off = fdt_read_u32(dtb, scan + 8) as usize;
                        let prop_name =
                            core::ffi::CStr::from_bytes_until_nul(&dtb[strings_off + name_off..])
                                .map(|s| s.to_str().unwrap_or(""))
                                .unwrap_or("");
                        let data = &dtb[scan + 12..scan + 12 + prop_len];
                        if depth == 1
                            && prop_name == "compatible"
                            && prop_len > 0
                            && (data.windows(12).any(|w| w == b"riscv,imsics")
                                || data.windows(11).any(|w| w == b"riscv,imsic"))
                        {
                            is_imsic = true;
                        }
                        if depth == 1
                            && prop_name == "interrupts-extended"
                            && fdt_interrupts_extended_has_irq(data, RISCV_MACHINE_EXTERNAL_IRQ)
                        {
                            has_m_level_irq = true;
                        }
                        scan += 12 + ((prop_len + 3) & !3);
                    } else if t == FDT_NOP {
                        scan += 4;
                    } else {
                        break;
                    }
                }

                if is_imsic && has_m_level_irq {
                    let node_name = alloc::string::String::from(name);
                    for i in (node_start..scan).step_by(4) {
                        fdt_write_u32(dtb, i, FDT_NOP);
                    }
                    info!("AIA: NOP'd M-level IMSIC node '{}' in DTB", node_name);
                }
            }
            t if t == FDT_END_NODE => {
                off += 4;
            }
            t if t == FDT_PROP => {
                let prop_len = fdt_read_u32(dtb, off + 4) as usize;
                off += 12 + ((prop_len + 3) & !3);
            }
            t if t == FDT_NOP => {
                off += 4;
            }
            _ => break,
        }
    }
}

fn fdt_nop_m_level_aplic(dtb: &mut [u8]) {
    let struct_off = fdt_read_u32(dtb, 8) as usize;
    let struct_size = fdt_read_u32(dtb, 36) as usize;
    let strings_off = fdt_read_u32(dtb, 12) as usize;
    let end = struct_off + struct_size;
    let mut off = struct_off;

    while off + 4 <= end {
        let token = fdt_read_u32(dtb, off);
        match token {
            t if t == FDT_BEGIN_NODE => {
                let name_start = off + 4;
                let name = core::ffi::CStr::from_bytes_until_nul(&dtb[name_start..])
                    .map(|s| s.to_str().unwrap_or(""))
                    .unwrap_or("");
                let node_start = off;
                let name_len = name.len();
                off += 4 + ((name_len + 4) & !3);

                let mut is_aplic = false;
                let mut has_delegation = false;
                let mut scan = off;
                let mut depth = 1u32;

                while depth > 0 && scan + 4 <= end {
                    let t = fdt_read_u32(dtb, scan);
                    if t == FDT_BEGIN_NODE {
                        depth += 1;
                        let n = core::ffi::CStr::from_bytes_until_nul(&dtb[scan + 4..])
                            .map(|s| s.to_str().unwrap_or(""))
                            .unwrap_or("");
                        scan += 4 + ((n.len() + 4) & !3);
                    } else if t == FDT_END_NODE {
                        depth -= 1;
                        scan += 4;
                    } else if t == FDT_PROP {
                        let prop_len = fdt_read_u32(dtb, scan + 4) as usize;
                        let name_off = fdt_read_u32(dtb, scan + 8) as usize;
                        let prop_name =
                            core::ffi::CStr::from_bytes_until_nul(&dtb[strings_off + name_off..])
                                .map(|s| s.to_str().unwrap_or(""))
                                .unwrap_or("");
                        if depth == 1 && prop_name == "compatible" && prop_len > 0 {
                            let data = &dtb[scan + 12..scan + 12 + prop_len];
                            if data.windows(11).any(|w| w == b"riscv,aplic") {
                                is_aplic = true;
                            }
                        }
                        if depth == 1
                            && (prop_name == "riscv,delegate" || prop_name == "riscv,delegation")
                        {
                            has_delegation = true;
                        }
                        scan += 12 + ((prop_len + 3) & !3);
                    } else if t == FDT_NOP {
                        scan += 4;
                    } else {
                        break;
                    }
                }

                if is_aplic && has_delegation {
                    let node_name = alloc::string::String::from(name);
                    for i in (node_start..scan).step_by(4) {
                        fdt_write_u32(dtb, i, FDT_NOP);
                    }
                    info!("AIA: NOP'd M-level APLIC node '{}' in DTB", node_name);
                }
            }
            t if t == FDT_END_NODE => {
                off += 4;
            }
            t if t == FDT_PROP => {
                let prop_len = fdt_read_u32(dtb, off + 4) as usize;
                off += 12 + ((prop_len + 3) & !3);
            }
            t if t == FDT_NOP => {
                off += 4;
            }
            _ => break,
        }
    }
}

static mut SBI_START_ADDRESS: usize = 0;
static mut SBI_END_ADDRESS: usize = 0;
static mut RODATA_START_ADDRESS: usize = 0;
static mut RODATA_END_ADDRESS: usize = 0;

pub fn set_pmp(memory_range: &Range<usize>) {
    unsafe {
        // [0..memory_range.start] RWX
        // [memory_range.start..sbi_start] RWX
        // [sbi_start..sbi_rodata_start] R
        // [sbi_rodata_start..sbi_rodata_end] NONE
        // [sbi_rodata_end..sbi_end] R
        // [sbi_end..memory_range.end] RWX
        // [memory_range.end..INF] RWX
        use riscv::register::*;

        asm!("la {}, sbi_start", out(reg) SBI_START_ADDRESS, options(nomem));
        asm!("la {}, sbi_end", out(reg) SBI_END_ADDRESS, options(nomem));
        asm!("la {}, sbi_rodata_start", out(reg) RODATA_START_ADDRESS, options(nomem));
        asm!("la {}, sbi_rodata_end", out(reg) RODATA_END_ADDRESS, options(nomem));

        assert_eq!(memory_range.start & 0x3, 0);
        assert_eq!(memory_range.end & 0x3, 0);
        assert_eq!(SBI_START_ADDRESS & 0x3, 0);
        assert_eq!(SBI_END_ADDRESS & 0x3, 0);
        assert_eq!(RODATA_START_ADDRESS & 0x3, 0);
        assert_eq!(RODATA_END_ADDRESS & 0x3, 0);

        // When AIA is active, block S-mode access to M-level interrupt
        // controller regions while keeping other low MMIO visible.
        // This matches OpenSBI's domain isolation approach.
        if crate::platform::aia::is_aia_active() {
            if let Some(ref aia_info) = crate::platform::PLATFORM.info.aia.as_ref() {
                const QEMU_VIRT_M_APLIC_BASE: usize = 0x0c00_0000;
                const QEMU_VIRT_CLINT_BASE: usize = 0x0200_0000;
                const QEMU_VIRT_APLIC_SIZE: usize = 0x8000;
                const QEMU_VIRT_CLINT_SIZE: usize = 0x1_0000;

                let clint_base = crate::platform::PLATFORM
                    .info
                    .ipi
                    .as_ref()
                    .map(|(base, _)| *base)
                    .unwrap_or(QEMU_VIRT_CLINT_BASE);
                let clint_end = clint_base + QEMU_VIRT_CLINT_SIZE;
                let aplic_base = QEMU_VIRT_M_APLIC_BASE;
                let aplic_end = aplic_base + QEMU_VIRT_APLIC_SIZE;
                let m_base = aia_info.layout.machine_base;
                let m_end = aia_info
                    .hart_imsic_map
                    .iter()
                    .flatten()
                    .copied()
                    .max()
                    .and_then(|addr| addr.checked_add(0x1000))
                    .unwrap_or(m_base + 0x1000);

                pmpcfg0::set_pmp(0, Range::OFF, Permission::NONE, false);
                pmpaddr0::write(0);
                pmpcfg0::set_pmp(1, Range::TOR, Permission::RWX, false);
                pmpaddr1::write(clint_base >> 2);
                pmpcfg0::set_pmp(2, Range::TOR, Permission::NONE, false);
                pmpaddr2::write(clint_end >> 2);
                pmpcfg0::set_pmp(3, Range::TOR, Permission::RWX, false);
                pmpaddr3::write(aplic_base >> 2);
                pmpcfg0::set_pmp(4, Range::TOR, Permission::NONE, false);
                pmpaddr4::write(aplic_end >> 2);
                pmpcfg0::set_pmp(5, Range::TOR, Permission::RWX, false);
                pmpaddr5::write(m_base >> 2);
                pmpcfg0::set_pmp(6, Range::TOR, Permission::NONE, false);
                pmpaddr6::write(m_end >> 2);
                pmpcfg0::set_pmp(7, Range::TOR, Permission::RWX, false);
                pmpaddr7::write(memory_range.start >> 2);
                pmpcfg2::set_pmp(0, Range::TOR, Permission::RWX, false);
                pmpaddr8::write(SBI_START_ADDRESS >> 2);
                pmpcfg2::set_pmp(1, Range::TOR, Permission::R, false);
                pmpaddr9::write(RODATA_START_ADDRESS >> 2);
                pmpcfg2::set_pmp(2, Range::TOR, Permission::NONE, false);
                pmpaddr10::write(RODATA_END_ADDRESS >> 2);
                pmpcfg2::set_pmp(3, Range::TOR, Permission::RW, false);
                pmpaddr11::write(SBI_END_ADDRESS >> 2);
                pmpcfg2::set_pmp(4, Range::TOR, Permission::RWX, false);
                pmpaddr12::write(memory_range.end >> 2);
                pmpcfg2::set_pmp(5, Range::TOR, Permission::RWX, false);
                pmpaddr13::write(usize::MAX >> 2);
                return;
            }
        }

        pmpcfg0::set_pmp(0, Range::OFF, Permission::NONE, false);
        pmpaddr0::write(0);
        pmpcfg0::set_pmp(1, Range::TOR, Permission::RWX, false);
        pmpaddr1::write(memory_range.start >> 2);
        pmpcfg0::set_pmp(2, Range::TOR, Permission::RWX, false);
        pmpaddr2::write(SBI_START_ADDRESS >> 2);
        pmpcfg0::set_pmp(3, Range::TOR, Permission::R, false);
        pmpaddr3::write(RODATA_START_ADDRESS >> 2);
        pmpcfg0::set_pmp(4, Range::TOR, Permission::NONE, false);
        pmpaddr4::write(RODATA_END_ADDRESS >> 2);
        pmpcfg0::set_pmp(5, Range::TOR, Permission::RW, false); // FIXME: Should be Permission::R, temporarily fix for possible S-mode DTB modification
        pmpaddr5::write(SBI_END_ADDRESS >> 2);
        pmpcfg0::set_pmp(6, Range::TOR, Permission::RWX, false);
        pmpaddr6::write(memory_range.end >> 2);
        pmpcfg0::set_pmp(7, Range::TOR, Permission::RWX, false);
        pmpaddr7::write(usize::MAX >> 2);
    }
}

/// For print PMP Permission.
#[repr(transparent)]
struct PermissionWrapper(pub Permission);

/// For print PMP Range.
#[repr(transparent)]
struct RangeWrapper(pub register::Range);

impl fmt::Display for PermissionWrapper {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.pad(match self.0 {
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
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.pad(match self.0 {
            register::Range::OFF => "OFF",
            register::Range::TOR => "TOR",
            register::Range::NA4 => "NA4",
            register::Range::NAPOT => "NAPOT",
        })
    }
}

pub fn log_pmp_cfg(_memory_range: &Range<usize>) {
    use riscv::register::*;
    let pmp = pmpcfg0::read();

    let get_pmp_range = |i: usize| -> RangeWrapper { RangeWrapper(pmp.into_config(i).range) };
    let get_pmp_permission =
        |i: usize| -> PermissionWrapper { PermissionWrapper(pmp.into_config(i).permission) };
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
