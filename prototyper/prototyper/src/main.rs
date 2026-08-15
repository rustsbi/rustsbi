#![feature(alloc_error_handler)]
#![feature(fn_align)]
#![no_std]
#![no_main]
#![allow(static_mut_refs)]

extern crate alloc;
#[macro_use]
extern crate log;
#[macro_use]
mod macros;

mod cfg;
mod devicetree;
mod fail;
mod firmware;
mod platform;
mod riscv;
mod sbi;

use core::arch::asm;
use core::sync::atomic::Ordering;

use crate::platform::{IS_K1_PLATFORM, PLATFORM};
use crate::riscv::csr::{CSR_MSTATEEN0, menvcfg, mstateen};
use crate::riscv::current_hartid;
use crate::riscv::spacemit_k1;
use crate::sbi::features::hart_mhpm_mask;
use crate::sbi::features::{
    Extension, PrivilegedVersion, hart_extension_probe, hart_features_detection,
    hart_privileged_check, hart_privileged_version,
};
use crate::sbi::hart_context::NextStage;
use crate::sbi::heap::sbi_heap_init;
use crate::sbi::hsm::local_remote_hsm;
use crate::sbi::ipi;
use crate::sbi::trap_stack;
use rustsbi_prototyper_macros::entry;

#[inline(always)]
fn has_mstateen0() -> bool {
    has_csr!(CSR_MSTATEEN0)
}

#[entry]
fn main(_hart_id: usize, opaque: usize, nonstandard_a2: usize) {
    // Track whether SBI is initialized and ready.

    // Get init hart
    let init_hart_info = firmware::get_work_hart(opaque, nonstandard_a2, false);

    // init hart task entry.
    if init_hart_info.is_boot_hart {
        // Initialize the sbi heap
        sbi_heap_init();

        // parse the device tree
        let fdt_address = init_hart_info.fdt_address;

        unsafe {
            PLATFORM.init(fdt_address);
            PLATFORM.print_board_info();
        }

        // SpacemiT K1 / Ky X1 early initialization.
        // IS_K1_PLATFORM was already recorded by Platform::init before the
        // ready flag was released, so it is visible here (and to secondary
        // harts once they observe ready()).
        if IS_K1_PLATFORM.load(Ordering::Acquire) {
            // Configure ML2SETUP for the boot hart
            spacemit_k1::cold_boot_allowed(current_hartid());

            unsafe {
                // Use the SBI link address as the warmboot entry
                let warmboot_addr = cfg::SBI_LINK_START_ADDRESS as u64;
                spacemit_k1::early_init(true, warmboot_addr);
            }
            info!("SpacemiT K1: early init done (MSETUP + CCI-550)");
        }

        firmware::set_pmp(unsafe { PLATFORM.info.memory_range.as_ref().unwrap() });
        firmware::log_pmp_cfg(unsafe { PLATFORM.info.memory_range.as_ref().unwrap() });

        // Log boot hart ID and PMP information
        let hart_id = current_hartid();
        info!("{:<30}: {}", "Boot HART ID", hart_id);

        // Detection Hart Features
        hart_features_detection();
        // Other harts task entry.
        trap_stack::prepare_for_trap();
        let priv_version = hart_privileged_version(hart_id);
        let mhpm_mask = hart_mhpm_mask(hart_id);
        info!(
            "{:<30}: {:?}",
            "Boot HART Privileged Version:", priv_version
        );
        info!("{:<30}: {:#08x}", "Boot HART MHPM Mask:", mhpm_mask);
    } else {
        // Detection Hart feature
        hart_features_detection();
        // Other harts task entry.
        trap_stack::prepare_for_trap();

        // Wait for boot hart to complete SBI initialization.
        while !unsafe { PLATFORM.ready() } {
            core::hint::spin_loop()
        }

        // SpacemiT K1: Configure ML2SETUP for this hart
        if IS_K1_PLATFORM.load(Ordering::Acquire) {
            spacemit_k1::cold_boot_allowed(current_hartid());
        }

        firmware::set_pmp(unsafe { PLATFORM.info.memory_range.as_ref().unwrap() });
    }

    // Get boot information and prepare for kernel entry.
    let boot_info = firmware::get_boot_info(nonstandard_a2);
    let (mpp, next_addr) = (boot_info.mpp, boot_info.next_address);

    // Check hart privileded.
    hart_privileged_check(mpp);

    let boot_hart_info = firmware::get_work_hart(opaque, nonstandard_a2, true);

    // boot hart task entry.
    if boot_hart_info.is_boot_hart {
        unsafe {
            PLATFORM.sbi_cpu_init_with_feature();
        }
        let fdt_address = boot_hart_info.fdt_address;
        let fdt_address = firmware::patch_device_tree(fdt_address);

        // Start kernel.
        local_remote_hsm().start(NextStage {
            start_addr: next_addr,
            next_mode: mpp,
            opaque: fdt_address,
        });

        info!(
            "Redirecting hart {} to {:#016x} in {:?} mode.",
            current_hartid(),
            next_addr,
            mpp
        );
    }

    // Clear all pending IPIs.
    ipi::clear_all();

    // Per-hart IMSIC initialization when AIA is active and Smaia is supported.
    if crate::platform::aia::is_aia_active() {
        let hart_id = crate::riscv::current_hartid();
        if crate::sbi::features::hart_extension_probe(
            hart_id,
            crate::sbi::features::Extension::Smaia,
        ) {
            if let Some(ref aia_info) = unsafe { PLATFORM.info.aia.as_ref() } {
                crate::platform::aia::imsic_init_hart(aia_info);
            }
        } else {
            warn!("Hart {} lacks Smaia, skipping IMSIC init", hart_id);
        }
    }

    // Configure CSRs
    unsafe {
        // Delegate all interrupts and exceptions to supervisor mode.
        asm!("csrw mideleg,    {}", in(reg) !0);
        asm!("csrw medeleg,    {}", in(reg) !0);
        asm!("csrw mcounteren, {}", in(reg) !0);
        asm!("csrw scounteren, {}", in(reg) !0);
        use ::riscv::register::{medeleg, mtvec};
        // Keep supervisor environment calls and illegal instructions in M-mode.
        medeleg::clear_supervisor_env_call();
        medeleg::clear_load_misaligned();
        medeleg::clear_store_misaligned();
        medeleg::clear_illegal_instruction();

        let hart_priv_version = hart_privileged_version(current_hartid());
        if hart_priv_version >= PrivilegedVersion::Version1_11 {
            asm!("csrw mcountinhibit, {}", in(reg) !0b111usize);
        }
        if hart_priv_version >= PrivilegedVersion::Version1_12 {
            // Configure environment features based on available extensions.
            if hart_extension_probe(current_hartid(), Extension::Sstc) {
                menvcfg::set_bits(
                    menvcfg::STCE | menvcfg::CBIE_INVALIDATE | menvcfg::CBCFE | menvcfg::CBZE,
                );
            } else {
                menvcfg::set_bits(menvcfg::CBIE_INVALIDATE | menvcfg::CBCFE | menvcfg::CBZE);
            }
            if crate::platform::aia::is_aia_active()
                && hart_extension_probe(current_hartid(), Extension::Smaia)
                && has_mstateen0()
            {
                mstateen::enable_smode_aia();
            }
        }
        // Set up trap handling.
        let val = mtvec::Mtvec::new(
            fast_trap::trap_entry as *const () as _,
            mtvec::TrapMode::Direct,
        );
        mtvec::write(val);
    }
}
