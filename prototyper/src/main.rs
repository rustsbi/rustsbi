#![feature(naked_functions, asm_const)]
#![no_std]
#![no_main]

#[macro_use]
extern crate log;
#[macro_use]
mod macros;

mod board;
mod clint;
mod console;
mod dt;
mod dynamic;
mod fail;
mod hart_context;
mod hsm;
mod reset;
mod rfence;
mod riscv_spec;
mod trap;
mod trap_stack;

use clint::ClintDevice;
use core::sync::atomic::{AtomicBool, Ordering};
use core::{arch::asm, mem::MaybeUninit};
use reset::TestDevice;
use riscv::register::mstatus;

use crate::board::{Board, SBI};
use crate::clint::SIFIVECLINT;
use crate::console::{ConsoleDevice, CONSOLE};
use crate::hsm::{local_remote_hsm, Hsm};
use crate::reset::RESET;
use crate::rfence::RFence;
use crate::riscv_spec::{current_hartid, menvcfg};
use crate::trap::trap_vec;
use crate::trap_stack::NUM_HART_MAX;

#[no_mangle]
extern "C" fn rust_main(_hart_id: usize, opaque: usize, nonstandard_a2: usize) {
    // parse dynamic information
    let info = dynamic::read_paddr(nonstandard_a2).unwrap_or_else(fail::no_dynamic_info_available);
    static GENESIS: AtomicBool = AtomicBool::new(true);

    let is_boot_hart = if info.boot_hart == usize::MAX {
        GENESIS.swap(false, Ordering::AcqRel)
    } else {
        current_hartid() == info.boot_hart
    };

    if is_boot_hart {
        let (mpp, next_addr) =
            dynamic::mpp_next_addr(&info).unwrap_or_else(fail::invalid_dynamic_data);

        // parse the device tree
        let dtb = dt::parse_device_tree(opaque).unwrap_or_else(fail::device_tree_format);
        let dtb = dtb.share();
        let tree =
            serde_device_tree::from_raw_mut(&dtb).unwrap_or_else(fail::device_tree_deserialize);

        // TODO: The device base address needs to be parsed from FDT
        reset::init(0x100000);
        console::init(0x10000000);
        clint::init(0x2000000);

        info!("RustSBI version {}", rustsbi::VERSION);
        rustsbi::LOGO.lines().for_each(|line| info!("{}", line));
        info!("Initializing RustSBI machine-mode environment.");
        if let Some(model) = tree.model {
            info!("Model: {}", model.iter().next().unwrap_or("<unspecified>"));
        }
        info!(
            "Chosen stdout item: {}",
            tree.chosen
                .stdout_path
                .iter()
                .next()
                .unwrap_or("<unspecified>")
        );

        // Init SBI
        unsafe {
            SBI = MaybeUninit::new(Board {
                uart16550: Some(ConsoleDevice::new(&CONSOLE)),
                clint: Some(ClintDevice::new(&SIFIVECLINT, NUM_HART_MAX)),
                hsm: Some(Hsm),
                sifive_test: Some(TestDevice::new(&RESET)),
                rfence: Some(RFence),
            });
        }

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
    }

    clint::clear();
    unsafe {
        asm!("csrw mideleg,    {}", in(reg) !0);
        asm!("csrw medeleg,    {}", in(reg) !0);
        asm!("csrw mcounteren, {}", in(reg) !0);
        use riscv::register::{medeleg, mtvec};
        medeleg::clear_supervisor_env_call();
        medeleg::clear_illegal_instruction();
        menvcfg::set_bits(
            menvcfg::STCE | menvcfg::CBIE_INVALIDATE | menvcfg::CBCFE | menvcfg::CBZE,
        );
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
        // clear bss segment
        "   la      t0, sbss
            la      t1, ebss
        1:  bgeu    t0, t1, 2f
            sd      zero, 0(t0)
            addi    t0, t0, 8
            j       1b",
        // prepare data segment
        "   la      t3, sidata
            la      t4, sdata
            la      t5, edata
        1:  bgeu    t4, t5, 2f
            ld      t6, 0(t3)
            sd      t6, 0(t4)
            addi    t3, t3, 8
            addi    t4, t4, 8
            j       1b",
        "2:",
         // 3. Prepare stack for each hart
        "   call    {locate_stack}",
        "   call    {main}",
        "   j       {hart_boot}",
        magic = const dynamic::MAGIC,
        locate_stack = sym trap_stack::locate,
        main         = sym rust_main,
        hart_boot         = sym trap::trap_vec,
        options(noreturn)
    )
}

#[derive(Debug)]
pub struct NextStage {
    start_addr: usize,
    opaque: usize,
    next_mode: mstatus::MPP,
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
