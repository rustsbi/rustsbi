#![cfg_attr(not(test), no_std)]
#![no_main]
#![allow(dead_code)]

#[macro_use]
extern crate axlog;

use axhal::mem::{MemRegionFlags, PhysAddr, memory_regions, phys_to_virt};

mod dtb;
mod log;
mod medium;
mod panic;
mod runtime;
mod shell;

#[cfg_attr(not(test), unsafe(no_mangle))]
pub extern "C" fn rust_main(_cpu_id: usize, dtb: usize) -> ! {
    axlog::init();
    axlog::set_max_level(option_env!("AX_LOG").unwrap_or("")); // no effect if set `log-level-*` features
    info!("Logging is enabled.");

    info!("Found physcial memory regions:");
    for r in axhal::mem::memory_regions() {
        info!(
            "  [{:x?}, {:x?}) {} ({:?})",
            r.paddr,
            r.paddr + r.size,
            r.name,
            r.flags
        );
    }

    #[cfg(feature = "alloc")]
    init_allocator();

    #[cfg(feature = "paging")]
    axmm::init_memory_management();

    info!("Initialize platform devices...");
    axhal::platform_init();

    #[cfg(any(feature = "fs", feature = "net"))]
    {
        #[allow(unused_variables)]
        let all_devices = axdriver::init_drivers();

        #[cfg(feature = "fs")]
        // 目前使用ramdisk的cpio格式时，驱动还不完善，用不了，需要注释掉
        // 如果使用virtio-blk驱动，则可以正常使用
        axfs::init_filesystems(all_devices.block);

        #[cfg(feature = "net")]
        axnet::init_network(all_devices.net);
    }
    ctor_bare::call_ctors();

    // Set to DTB_ADDRESS
    unsafe {
        dtb::GLOBAL_NOW_DTB_ADDRESS = phys_to_virt(PhysAddr::from_usize(dtb)).as_usize();
    }

    // if dtb is needed to next stage
    /*
    unsafe {
        let mut parser = dtb::DtbParser::new(phys_to_virt(PhysAddr::from_usize(dtb)).as_usize()).unwrap();
        parser.dump_all();
        if parser.modify_property(
            "/chosen",
            "bootargs",
            "console=ttyS0,115200 root=/dev/mmcblk0p2 rw rootwait",
        ) {
            error!("modify error!");
        }
        let new_dtb: usize = parser.save_to_mem();
        // Send 'new_dtb' to next stage
    }
    */

    crate::shell::shell_main();

    info!("will shut down.");

    axhal::misc::terminate();
}

fn init_allocator() {
    info!("Initialize global memory allocator...");
    info!("  use {} allocator.", axalloc::global_allocator().name());

    let mut max_region_size = 0;
    let mut max_region_paddr = 0.into();
    for r in memory_regions() {
        if r.flags.contains(MemRegionFlags::FREE) && r.size > max_region_size {
            max_region_size = r.size;
            max_region_paddr = r.paddr;
        }
    }
    for r in memory_regions() {
        if r.flags.contains(MemRegionFlags::FREE) && r.paddr == max_region_paddr {
            axalloc::global_init(phys_to_virt(r.paddr).as_usize(), r.size);
            break;
        }
    }
    for r in memory_regions() {
        if r.flags.contains(MemRegionFlags::FREE) && r.paddr != max_region_paddr {
            axalloc::global_add_memory(phys_to_virt(r.paddr).as_usize(), r.size)
                .expect("add heap memory region failed");
        }
    }
}
