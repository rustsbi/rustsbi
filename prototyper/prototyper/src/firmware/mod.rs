cfg_if::cfg_if! {
    if #[cfg(feature = "payload")] {
        pub mod payload;
        pub use payload::{get_boot_info, is_boot_hart};
    } else if #[cfg(feature = "jump")] {
        pub mod jump;
        pub use jump::{get_boot_info, is_boot_hart};
    } else {
        pub mod dynamic;
        pub use dynamic::{get_boot_info, is_boot_hart};
    }
}

use crate::fail;
use alloc::{format, vec};
#[allow(unused)]
use core::arch::{asm, naked_asm};
use core::{ops::Range, usize};

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
#[allow(unused_mut, unused_assignments)]
pub fn get_boot_hart(opaque: usize, nonstandard_a2: usize) -> BootHart {
    let is_boot_hart = is_boot_hart(nonstandard_a2);

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
    let original_length = ptr.align();
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
    let raw_list = [patch1, patch2];
    // Only add `reserved-memory` section when it not exists.
    let list = if tree.find("/reserved-memory").is_some() {
        &raw_list[1..]
    } else {
        &raw_list[..]
    };

    // Firstly, allocate a temporary buffer to store the fdt and get the real total size of the patched fdt.
    // TODO: The serde_device_tree can provide a function to calculate the accuracy size of patched fdt.
    debug!(
        "Allocate temporary DTB buffer with length 0x{:x}.",
        original_length + 2048
    );
    let mut temporary_buffer = vec![0u8; original_length + 2048];
    serde_device_tree::ser::to_dtb(&tree, &list, &mut temporary_buffer).unwrap();
    let Ok(patched_dtb_ptr) = DtbPtr::from_raw(temporary_buffer.as_mut_ptr()) else {
        panic!("Failed to parse the patched dtb.")
    };
    let patched_length = patched_dtb_ptr.align();

    // Secondly, allocate the exactly buffer to store the fdt.
    let mut patched_dtb_buffer = vec![0u8; patched_length];
    serde_device_tree::ser::to_dtb(&tree, &list, &mut patched_dtb_buffer).unwrap();
    // Intentionally leak the buffer so that the patched DTB remains valid for the lifetime of the firmware.
    // This is required because the returned pointer is used elsewhere and must not be deallocated.
    let patched_dtb = patched_dtb_buffer.leak();
    info!(
        "The patched dtb is located at 0x{:x} with length 0x{:x}.",
        patched_dtb.as_ptr() as usize,
        patched_length
    );
    patched_dtb.as_ptr() as usize
}

static mut SBI_START_ADDRESS: usize = 0;
static mut SBI_END_ADDRESS: usize = 0;
static mut RODATA_START_ADDRESS: usize = 0;
static mut RODATA_END_ADDRESS: usize = 0;

pub fn set_pmp(memory_range: &Range<usize>) {
    unsafe {
        // [0..memory_range.start] RWX
        // [memory_range.start..sbi_start] RWX
        // [sbi_start..sbi_rodata_start] NONE
        // [sbi_rodata_start..sbi_rodata_end] NONE
        // [sbi_rodata_end..sbi_end] NONE
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
        pmpcfg0::set_pmp(5, Range::TOR, Permission::R, false);
        pmpaddr5::write(SBI_END_ADDRESS >> 2);
        pmpcfg0::set_pmp(6, Range::TOR, Permission::RWX, false);
        pmpaddr6::write(memory_range.end >> 2);
        pmpcfg0::set_pmp(7, Range::TOR, Permission::RWX, false);
        pmpaddr7::write(usize::MAX >> 2);
    }
}

pub fn log_pmp_cfg(memory_range: &Range<usize>) {
    unsafe {
        info!("PMP Configuration");

        info!(
            "{:<10} {:<10} {:<15} {:<30}",
            "PMP", "Range", "Permission", "Address"
        );

        info!("{:<10} {:<10} {:<15} 0x{:08x}", "PMP 0:", "OFF", "NONE", 0);
        info!(
            "{:<10} {:<10} {:<15} 0x{:08x} - 0x{:08x}",
            "PMP 1-2:", "TOR", "RWX/RWX", memory_range.start, SBI_START_ADDRESS
        );
        info!(
            "{:<10} {:<10} {:<15} 0x{:08x} - 0x{:08x} - 0x{:08x}",
            "PMP 3-5:",
            "TOR",
            "NONE/NONE",
            RODATA_START_ADDRESS,
            RODATA_END_ADDRESS,
            SBI_END_ADDRESS
        );
        info!(
            "{:<10} {:<10} {:<15} 0x{:08x}",
            "PMP 6:", "TOR", "RWX", memory_range.end
        );
        info!(
            "{:<10} {:<10} {:<15} 0x{:08x}",
            "PMP 7:",
            "TOR",
            "RWX",
            usize::MAX
        );
    }
}
