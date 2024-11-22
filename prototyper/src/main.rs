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

use core::arch::asm;
use core::sync::atomic::{AtomicBool, Ordering};

use sbi::extensions;

use crate::board::{MachineConsoleType, BOARD};
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
use crate::sbi::Sbi;

pub const START_ADDRESS: usize = 0x80000000;
pub const R_RISCV_RELATIVE: usize = 3;

#[no_mangle]
extern "C" fn rust_main(_hart_id: usize, opaque: usize, nonstandard_a2: usize) {
    // Track whether SBI is initialized and ready.
    static SBI_READY: AtomicBool = AtomicBool::new(false);

    let boot_hart_info = platform::get_boot_hart(opaque, nonstandard_a2);
    // boot hart task entry.
    if boot_hart_info.is_boot_hart {
        // 1. Init FDT
        // parse the device tree
        // TODO: shoule remove `fail:device_tree_format`
        let fdt_addr = boot_hart_info.fdt_address;
        let dtb = dt::parse_device_tree(fdt_addr).unwrap_or_else(fail::device_tree_format);
        let dtb = dtb.share();

        // TODO: should remove `fail:device_tree_deserialize`.
        let root: serde_device_tree::buildin::Node = serde_device_tree::from_raw_mut(&dtb).unwrap();
        let tree =
            serde_device_tree::from_raw_mut(&dtb).unwrap_or_else(fail::device_tree_deserialize);
        // 2. Init device
        // TODO: The device base address should be find in a better way.
        'console_finder: for console_path in tree.chosen.stdout_path.iter() {
            if let Some(node) = root.find(console_path) {
                let compatible = node
                    .props()
                    .map(|mut prop_iter| {
                        prop_iter
                            .find(|prop_item| prop_item.get_name() == "compatible")
                            .map(|prop_item| {
                                prop_item.deserialize::<serde_device_tree::buildin::StrSeq>()
                            })
                    })
                    .map_or_else(|| None, |v| v);
                let regs = node
                    .props()
                    .map(|mut prop_iter| {
                        prop_iter
                            .find(|prop_item| prop_item.get_name() == "reg")
                            .map(|prop_item| {
                                let reg =
                                    prop_item.deserialize::<serde_device_tree::buildin::Reg>();
                                if let Some(range) = reg.iter().next() {
                                    return Some(range);
                                }
                                None
                            })
                            .map_or_else(|| None, |v| v)
                    })
                    .map_or_else(|| None, |v| v);
                if compatible.is_some() && regs.is_some() {
                    for device_id in compatible.unwrap().iter() {
                        if device_id == "ns16550a" {
                            board::console_dev_init(
                                MachineConsoleType::Uart16550,
                                regs.unwrap().0.start,
                            );
                            break 'console_finder;
                        }
                        if device_id == "xlnx,xps-uartlite-1.00.a" {
                            board::console_dev_init(
                                MachineConsoleType::UartAxiLite,
                                regs.unwrap().0.start,
                            );
                            break 'console_finder;
                        }
                    }
                }
            }
        }

        let clint_device = tree.soc.clint.unwrap().iter().next().unwrap();
        let cpu_num = tree.cpus.cpu.len();
        let ipi_base_address = clint_device.at();

        // Initialize reset device if present.
        if let Some(test) = tree.soc.test {
            let reset_device = test.iter().next().unwrap();
            let reset_base_address = reset_device.at();
            board::reset_dev_init(usize::from_str_radix(reset_base_address, 16).unwrap());
        }

        // Initialize console and IPI devices.
        board::ipi_dev_init(usize::from_str_radix(ipi_base_address, 16).unwrap());

        // 3. Init the SBI implementation
        // TODO: More than one memory node or range?
        let memory_reg = tree
            .memory
            .iter()
            .next()
            .unwrap()
            .deserialize::<dt::Memory>()
            .reg;
        let memory_range = memory_reg.iter().next().unwrap().0;

        // 3. Init SBI
        unsafe {
            BOARD.device.memory_range = Some(memory_range);
            BOARD.sbi = Sbi {
                console: Some(SbiConsole::new(BOARD.device.uart.as_ref().unwrap())),
                ipi: Some(SbiIpi::new(&BOARD.device.sifive_clint, cpu_num)),
                hsm: Some(SbiHsm),
                reset: Some(SbiReset::new(&BOARD.device.sifive_test)),
                rfence: Some(SbiRFence),
            };
        }

        // Setup trap handling.
        trap_stack::prepare_for_trap();
        extensions::init(&tree.cpus.cpu);
        SBI_READY.swap(true, Ordering::AcqRel);

        // 4. Init Logger
        logger::Logger::init().unwrap();

        info!("RustSBI version {}", rustsbi::VERSION);
        rustsbi::LOGO.lines().for_each(|line| info!("{}", line));
        info!("Initializing RustSBI machine-mode environment.");

        info!("Number of CPU: {}", cpu_num);
        if let Some(model) = tree.model {
            info!("Model: {}", model.iter().next().unwrap_or("<unspecified>"));
        }
        info!("Clint device: {}", ipi_base_address);
        info!(
            "Chosen stdout item: {}",
            tree.chosen
                .stdout_path
                .iter()
                .next()
                .unwrap_or("<unspecified>")
        );

        platform::set_pmp(unsafe { BOARD.device.memory_range.as_ref().unwrap() });

        // Get boot information and prepare for kernel entry.
        let boot_info = platform::get_boot_info(nonstandard_a2);
        let (mpp, next_addr) = (boot_info.mpp, boot_info.next_address);

        // Start kernel.
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
        // 设置陷入栈
        trap_stack::prepare_for_trap();

        // Wait for boot hart to complete SBI initialization.
        while !SBI_READY.load(Ordering::Relaxed) {
            core::hint::spin_loop()
        }

        platform::set_pmp(unsafe { BOARD.device.memory_range.as_ref().unwrap() });
    }

    // Clear all pending IPIs.
    ipi::clear_all();

    // Configure CSRs and trap handling.
    unsafe {
        // Delegate all interrupts and exceptions to supervisor mode.
        asm!("csrw mideleg,    {}", in(reg) !0);
        asm!("csrw medeleg,    {}", in(reg) !0);
        asm!("csrw mcounteren, {}", in(reg) !0);
        use riscv::register::{medeleg, mtvec};
        // Keep supervisor environment calls and illegal instructions in M-mode.
        medeleg::clear_supervisor_env_call();
        medeleg::clear_illegal_instruction();
        // Configure environment features based on available extensions.
        if hart_extension_probe(current_hartid(), Extension::Sstc) {
            menvcfg::set_bits(
                menvcfg::STCE | menvcfg::CBIE_INVALIDATE | menvcfg::CBCFE | menvcfg::CBZE,
            );
        } else {
            menvcfg::set_bits(menvcfg::CBIE_INVALIDATE | menvcfg::CBCFE | menvcfg::CBZE);
        }
        // Set up vectored trap handling.
        mtvec::write(trap_vec as _, mtvec::TrapMode::Vectored);
    }
}

#[naked]
#[link_section = ".text.entry"]
#[export_name = "_start"]
unsafe extern "C" fn start() -> ! {
    core::arch::asm!(
        // 1. Turn off interrupt.
        "   csrw    mie, zero",
        // 2. Initialize programming langauge runtime.
        // only clear bss if hartid matches preferred boot hart id.
        "   csrr    t0, mhartid",
        "   bne     t0, zero, 4f",
        "   call    {relocation_update}",
        "1:",
        // 3. Hart 0 clear bss segment.
        "   lla     t0, sbss
            lla     t1, ebss
         2: bgeu    t0, t1, 3f
            sd      zero, 0(t0)
            addi    t0, t0, 8
            j       2b",
        "3: ", // Hart 0 set bss ready signal.
        "   lla     t0, 6f
            li      t1, 1
            amoadd.w t0, t1, 0(t0)
            j       5f",
        "4:", // Other harts are waiting for bss ready signal.
        "   li      t1, 1
            lla     t0, 6f
            lw      t0, 0(t0)
            bne     t0, t1, 4b", 
        "5:",
         // 4. Prepare stack for each hart.
        "   call    {locate_stack}",
        "   call    {main}",
        "   csrw    mscratch, sp",
        "   j       {hart_boot}",
        "  .balign  4",
        "6:",  // bss ready signal.
        "  .word    0",
        relocation_update = sym relocation_update,
        locate_stack = sym trap_stack::locate,
        main         = sym rust_main,
        hart_boot    = sym trap::msoft,
        options(noreturn)
    )
}

// Handle relocations for position-independent code
#[naked]
unsafe extern "C" fn relocation_update() {
    asm!(
        // Get load offset.
        "   li t0, {START_ADDRESS}",
        "   lla t1, sbi_start",
        "   sub t2, t1, t0",

        // Foreach rela.dyn and update relocation.
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
