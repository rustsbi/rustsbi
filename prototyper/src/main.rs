#![feature(naked_functions)]
#![no_std]
#![no_main]
#![allow(static_mut_refs)]

#[macro_use]
extern crate log;
#[macro_use]
mod macros;

mod board;
mod dt;
mod fail;
mod platform;
mod riscv_spec;
mod sbi;

use core::sync::atomic::{AtomicBool, Ordering};
use core::{arch::asm, mem::MaybeUninit};

use sbi::extensions;

use crate::board::{SBI_IMPL, SIFIVECLINT, SIFIVETEST, UART};
use crate::riscv_spec::{current_hartid, menvcfg};
use crate::sbi::console::SbiConsole;
use crate::sbi::extensions::{hart_extension_probe, Extension};
use crate::sbi::hart_context::NextStage;
use crate::sbi::hsm::{local_remote_hsm, SbiHsm};
use crate::sbi::ipi::{self, SbiIpi};
use crate::sbi::logger;
use crate::sbi::reset::SbiReset;
use crate::sbi::rfence::SbiRFence;
use crate::sbi::trap::{self, trap_vec};
use crate::sbi::trap_stack;
use crate::sbi::SBI;

pub const START_ADDRESS: usize = 0x80000000;
pub const R_RISCV_RELATIVE: usize = 3;

#[no_mangle]
extern "C" fn rust_main(_hart_id: usize, opaque: usize, nonstandard_a2: usize) {
    // parse dynamic information
    static SBI_READY: AtomicBool = AtomicBool::new(false);

    let boot_hart_info = platform::get_boot_hart(opaque, nonstandard_a2);
    // boot hart task entry
    if boot_hart_info.is_boot_hart {
        let fdt_addr = boot_hart_info.fdt_address;

        // 1. Init FDT
        // parse the device tree
        // TODO: shoule remove `fail:device_tree_format`
        let dtb = dt::parse_device_tree(fdt_addr).unwrap_or_else(fail::device_tree_format);
        let dtb = dtb.share();

        // TODO: should remove `fail:device_tree_deserialize`
        let tree =
            serde_device_tree::from_raw_mut(&dtb).unwrap_or_else(fail::device_tree_deserialize);

        // 2. Init device
        // TODO: The device base address should be find in a better way
        let console_base = tree.soc.serial.unwrap().iter().next().unwrap();
        let clint_device = tree.soc.clint.unwrap().iter().next().unwrap();
        let cpu_num = tree.cpus.cpu.len();
        let console_base_address = console_base.at();
        let ipi_base_address = clint_device.at();

        // Set reset device if found it
        if let Some(test) = tree.soc.test {
            let reset_device = test.iter().next().unwrap();
            let reset_base_address = reset_device.at();
            board::reset_dev_init(usize::from_str_radix(reset_base_address, 16).unwrap());
        }

        board::console_dev_init(usize::from_str_radix(console_base_address, 16).unwrap());
        board::ipi_dev_init(usize::from_str_radix(ipi_base_address, 16).unwrap());

        // 3. Init SBI
        unsafe {
            SBI_IMPL = MaybeUninit::new(SBI {
                console: Some(SbiConsole::new(&UART)),
                ipi: Some(SbiIpi::new(&SIFIVECLINT, cpu_num)),
                hsm: Some(SbiHsm),
                reset: Some(SbiReset::new(&SIFIVETEST)),
                rfence: Some(SbiRFence),
            });
        }
        // 设置陷入栈
        trap_stack::prepare_for_trap();
        extensions::init(&tree.cpus.cpu);
        SBI_READY.swap(true, Ordering::AcqRel);
        // 4. Init Logger
        logger::Logger::init();

        info!("RustSBI version {}", rustsbi::VERSION);
        rustsbi::LOGO.lines().for_each(|line| info!("{}", line));
        info!("Initializing RustSBI machine-mode environment.");

        info!("Number of CPU: {}", cpu_num);
        if let Some(model) = tree.model {
            info!("Model: {}", model.iter().next().unwrap_or("<unspecified>"));
        }
        info!("Clint device: {}", ipi_base_address);
        info!("Console deivce: {}", console_base_address);
        info!(
            "Chosen stdout item: {}",
            tree.chosen
                .stdout_path
                .iter()
                .next()
                .unwrap_or("<unspecified>")
        );

        // TODO: PMP configuration needs to be obtained through the memory range in the device tree
        use riscv::register::*;
        unsafe {
            pmpcfg0::set_pmp(0, Range::OFF, Permission::NONE, false);
            pmpaddr0::write(0);
            pmpcfg0::set_pmp(1, Range::TOR, Permission::RWX, false);
            pmpaddr1::write(usize::MAX >> 2);
        }

        let boot_info = platform::get_boot_info(nonstandard_a2);
        let (mpp, next_addr) = (boot_info.mpp, boot_info.next_address);
        // 设置内核入口
        local_remote_hsm().start(NextStage {
            start_addr: next_addr,
            next_mode: mpp,
            opaque: fdt_addr,
        });

        info!(
            "Redirecting hart {} to 0x{:0>16x} in {:?} mode.",
            current_hartid(),
            next_addr,
            mpp
        );
    } else {
        // TODO: PMP configuration needs to be obtained through the memory range in the device tree
        use riscv::register::*;
        unsafe {
            pmpcfg0::set_pmp(0, Range::OFF, Permission::NONE, false);
            pmpaddr0::write(0);
            pmpcfg0::set_pmp(1, Range::TOR, Permission::RWX, false);
            pmpaddr1::write(usize::MAX >> 2);
        }

        // 设置陷入栈
        trap_stack::prepare_for_trap();

        // waiting for sbi ready
        while !SBI_READY.load(Ordering::Relaxed) {
            core::hint::spin_loop()
        }
    }

    ipi::clear_all();
    unsafe {
        asm!("csrw mideleg,    {}", in(reg) !0);
        asm!("csrw medeleg,    {}", in(reg) !0);
        asm!("csrw mcounteren, {}", in(reg) !0);
        use riscv::register::{medeleg, mtvec};
        medeleg::clear_supervisor_env_call();
        medeleg::clear_illegal_instruction();
        if hart_extension_probe(current_hartid(), Extension::Sstc) {
            menvcfg::set_bits(
                menvcfg::STCE | menvcfg::CBIE_INVALIDATE | menvcfg::CBCFE | menvcfg::CBZE,
            );
        } else {
            menvcfg::set_bits(menvcfg::CBIE_INVALIDATE | menvcfg::CBCFE | menvcfg::CBZE);
        }
        mtvec::write(trap_vec as _, mtvec::TrapMode::Vectored);
    }
}

#[naked]
#[link_section = ".text.entry"]
#[export_name = "_start"]
unsafe extern "C" fn start() -> ! {
    core::arch::asm!(
        // 1. Turn off interrupt
        "   csrw    mie, zero",
        // 2. Initialize programming langauge runtime
        // only clear bss if hartid matches preferred boot hart id
        "   csrr    t0, mhartid",
        "   bne     t0, zero, 4f",
        "   call    {relocation_update}",
        "1:",
        // 3. Hart 0 clear bss segment
        "   lla     t0, sbss
            lla     t1, ebss
         2: bgeu    t0, t1, 3f
            sd      zero, 0(t0)
            addi    t0, t0, 8
            j       2b",
        "3: ", // Hart 0 set bss ready signal
        "   lla     t0, 6f
            li      t1, 1
            amoadd.w t0, t1, 0(t0)
            j       5f",
        "4:", // Other harts are waiting for bss ready signal
        "   li      t1, 1
            lla     t0, 6f
            lw      t0, 0(t0)
            bne     t0, t1, 4b", 
        "5:",
         // 4. Prepare stack for each hart
        "   call    {locate_stack}",
        "   call    {main}",
        "   csrw    mscratch, sp",
        "   j       {hart_boot}",
        "  .balign  4",
        "6:",  // bss ready signal
        "  .word    0",
        relocation_update = sym relocation_update,
        locate_stack = sym trap_stack::locate,
        main         = sym rust_main,
        hart_boot    = sym trap::msoft,
        options(noreturn)
    )
}

#[naked]
unsafe extern "C" fn relocation_update() {
    asm!(
        // Get load offset
        "   li t0, {START_ADDRESS}",
        "   lla t1, .text.entry",
        "   sub t2, t1, t0",

        // Foreach rela.dyn and update relocation
        "   lla t0, __rel_dyn_start",
        "   lla t1, __rel_dyn_end",
        "   li  t3, {R_RISCV_RELATIVE}",
        "1:",
        "   ld  t4, 8(t0)",
        "   bne t4, t3, 2f",
        "   ld t4, 0(t0)", // Get offset
        "   ld t5, 16(t0)", // Get append
        "   add t4, t4, t2", // Add load offset to offset add append
        "   add t5, t5, t2",
        "   sd t5, 0(t4)", // Update address
        "   addi t0, t0, 24", // Get next rela item
        "2:",
        "   blt t0, t1, 1b",

        // Return
        "   ret",
        R_RISCV_RELATIVE = const R_RISCV_RELATIVE,
        START_ADDRESS = const START_ADDRESS,
        options(noreturn)
    )
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    use riscv::register::*;
    println!(
        "[rustsbi-panic] hart {} {info}",
        riscv::register::mhartid::read()
    );
    println!(
        "-----------------------------
> mcause:  {:?}
> mepc:    {:#018x}
> mtval:   {:#018x}
-----------------------------",
        mcause::read().cause(),
        mepc::read(),
        mtval::read()
    );
    println!("[rustsbi-panic] system shutdown scheduled due to RustSBI panic");
    loop {}
}
