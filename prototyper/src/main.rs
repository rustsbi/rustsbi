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
mod dynamic;
mod fail;
mod riscv_spec;
mod sbi;

use core::sync::atomic::{AtomicBool, Ordering};
use core::{arch::asm, mem::MaybeUninit};

use crate::board::{SBI_IMPL, SIFIVECLINT, SIFIVETEST, UART};
use crate::riscv_spec::{current_hartid, menvcfg};
use crate::sbi::console::SbiConsole;
use crate::sbi::hart_context::NextStage;
use crate::sbi::hsm::{local_remote_hsm, SbiHsm};
use crate::sbi::ipi::{self, SbiIpi};
use crate::sbi::logger;
use crate::sbi::reset::SbiReset;
use crate::sbi::rfence::SbiRFence;
use crate::sbi::trap::{self, trap_vec};
use crate::sbi::trap_stack::{self, NUM_HART_MAX};
use crate::sbi::SBI;

#[no_mangle]
extern "C" fn rust_main(_hart_id: usize, opaque: usize, nonstandard_a2: usize) {
    // parse dynamic information
    let info = dynamic::read_paddr(nonstandard_a2).unwrap_or_else(fail::no_dynamic_info_available);
    static GENESIS: AtomicBool = AtomicBool::new(true);
    static SBI_READY: AtomicBool = AtomicBool::new(false);

    let is_boot_hart = if info.boot_hart == usize::MAX {
        GENESIS.swap(false, Ordering::AcqRel)
    } else {
        current_hartid() == info.boot_hart
    };

    if is_boot_hart {
        let (mpp, next_addr) =
            dynamic::mpp_next_addr(&info).unwrap_or_else(fail::invalid_dynamic_data);

        // parse the device tree

        // 1. Init FDT
        let dtb = dt::parse_device_tree(opaque).unwrap_or_else(fail::device_tree_format);
        let dtb = dtb.share();
        let tree =
            serde_device_tree::from_raw_mut(&dtb).unwrap_or_else(fail::device_tree_deserialize);

        // 2. Init device
        // TODO: The device base address should be find in a better way
        let reset_device = tree.soc.test.unwrap().iter().next().unwrap();
        let console_base = tree.soc.serial.unwrap().iter().next().unwrap();
        let clint_device = tree.soc.clint.unwrap().iter().next().unwrap();
        let reset_base_address = reset_device.at();
        let console_base_address = console_base.at();
        let ipi_base_address = clint_device.at();
        board::reset_dev_init(usize::from_str_radix(reset_base_address, 16).unwrap());
        board::console_dev_init(usize::from_str_radix(console_base_address, 16).unwrap());
        board::ipi_dev_init(usize::from_str_radix(ipi_base_address, 16).unwrap());
        // Assume sstc is enabled only if all hart has sstc ext
        let sstc_support = tree
            .cpus
            .cpu
            .iter()
            .map(|cpu_iter| {
                use crate::dt::Cpu;
                let cpu = cpu_iter.deserialize::<Cpu>();
                let isa = match cpu.isa {
                    Some(value) => value,
                    None => return false,
                };
                isa.iter().find(|&x| x == "sstc").is_some()
            })
            .all(|x| x);
        // 3. Init SBI
        unsafe {
            SBI_IMPL = MaybeUninit::new(SBI {
                console: Some(SbiConsole::new(&UART)),
                ipi: Some(SbiIpi::new(&SIFIVECLINT, NUM_HART_MAX, sstc_support)),
                hsm: Some(SbiHsm),
                reset: Some(SbiReset::new(&SIFIVETEST)),
                rfence: Some(SbiRFence),
            });
        }
        SBI_READY.swap(true, Ordering::AcqRel);
        // 4. Init Logger
        logger::Logger::init();
        info!("RustSBI version {}", rustsbi::VERSION);
        rustsbi::LOGO.lines().for_each(|line| info!("{}", line));
        info!("Initializing RustSBI machine-mode environment.");

        if let Some(model) = tree.model {
            info!("Model: {}", model.iter().next().unwrap_or("<unspecified>"));
        }
        info!("Support sstc: {sstc_support}");
        info!("Clint device: {}", ipi_base_address);
        info!("Console deivce: {}", console_base_address);
        info!("Reset device: {}", reset_base_address);
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

        // 设置陷入栈
        trap_stack::prepare_for_trap();

        // 设置内核入口
        local_remote_hsm().start(NextStage {
            start_addr: next_addr,
            next_mode: mpp,
            opaque,
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
        while !SBI_READY.load(Ordering::Relaxed) {}
    }

    ipi::clear_all();
    unsafe {
        asm!("csrw mideleg,    {}", in(reg) !0);
        asm!("csrw medeleg,    {}", in(reg) !0);
        asm!("csrw mcounteren, {}", in(reg) !0);
        use riscv::register::{medeleg, mtvec};
        medeleg::clear_supervisor_env_call();
        medeleg::clear_illegal_instruction();
        if ipi::has_sstc() {
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
        "   ld      t1, 0(a2)",
        "   li      t2, {magic}",
        "   bne     t1, t2, 3f",
        "   ld      t2, 40(a2)",
        "   bne     t0, t2, 2f",
        "   j       4f",
        "3:",
        "   j       3b", // TODO multi hart preempt for runtime init
        "4:",
        // 3. clear bss segment
        "   la      t0, sbss
            la      t1, ebss
        1:  bgeu    t0, t1, 2f
            sd      zero, 0(t0)
            addi    t0, t0, 8
            j       1b",
        "2:",
         // 4. Prepare stack for each hart
        "   call    {locate_stack}",
        "   call    {main}",
        "   csrw    mscratch, sp",
        "   j       {hart_boot}",
        magic = const dynamic::MAGIC,
        locate_stack = sym trap_stack::locate,
        main         = sym rust_main,
        hart_boot    = sym trap::msoft,
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
