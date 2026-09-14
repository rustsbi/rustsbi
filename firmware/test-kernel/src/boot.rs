//! Test-kernel entry points and top-level test orchestration.
//!
//! The individual suites live in sibling modules.  Keeping this module to
//! boot-time setup and the final result decision makes the execution order
//! visible without hiding suite details in a single large function.

use core::arch::{asm, naked_asm};

use sbi_testing::sbi;

use crate::{console, misaligned, platform, pmu, reset, rfence, trap};

const RISCV_HEAD_FLAGS: u64 = 0;
const RISCV_HEADER_VERSION: u32 = 0x2;
const RISCV_IMAGE_MAGIC: u64 = 0x5643534952; // Magic number, little endian, "RISCV".
const RISCV_IMAGE_MAGIC2: u32 = 0x05435352; // Magic number 2, little endian, "RSC\x05".

/// RISC-V Linux image boot header.
#[unsafe(naked)]
#[unsafe(no_mangle)]
#[unsafe(link_section = ".head.text")]
unsafe extern "C" fn _boot_header() -> ! {
    naked_asm!(
        "j _start",
        ".word 0",
        ".balign 8",
        ".dword 0x200000",
        ".dword iend - istart",
        ".dword {RISCV_HEAD_FLAGS}",
        ".word  {RISCV_HEADER_VERSION}",
        ".word  0",
        ".dword 0",
        ".dword {RISCV_IMAGE_MAGIC}",
        ".balign 4",
        ".word  {RISCV_IMAGE_MAGIC2}",
        ".word  0",
        RISCV_HEAD_FLAGS = const RISCV_HEAD_FLAGS,
        RISCV_HEADER_VERSION = const RISCV_HEADER_VERSION,
        RISCV_IMAGE_MAGIC = const RISCV_IMAGE_MAGIC,
        RISCV_IMAGE_MAGIC2 = const RISCV_IMAGE_MAGIC2,
    );
}

/// Enters Rust after clearing `.bss` and installing the boot hart's stack.
#[unsafe(naked)]
#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.entry")]
unsafe extern "C" fn _start(hartid: usize, device_tree_paddr: usize) -> ! {
    const STACK_SIZE: usize = 16384; // 16 KiB.

    #[unsafe(link_section = ".bss.uninit")]
    static mut STACK: [u8; STACK_SIZE] = [0u8; STACK_SIZE];

    naked_asm!(
        // Clear the zero-initialized data segment before Rust starts.
        "   la      t0, sbss
            la      t1, ebss
        1:  bgeu    t0, t1, 2f
            .if {XLEN} == 64
            sd      zero, 0(t0)
            addi    t0, t0, 8
            .else
            sw      zero, 0(t0)
            addi    t0, t0, 4
            .endif
            j       1b",
        "2:",
        "   la sp, {stack} + {stack_size}",
        "   j  {main}",
        stack_size = const STACK_SIZE,
        stack = sym STACK,
        main = sym rust_main,
        XLEN = const usize::BITS,
    )
}

/// Runs the test suites in their stable boot order and requests shutdown.
extern "C" fn rust_main(hartid: usize, dtb_pa: usize) -> ! {
    let platform::BoardInfo { smp, frequency } = platform::initialize(dtb_pa);
    print_banner(hartid, smp, frequency, dtb_pa);

    let test_result = sbi_testing::Testing {
        hartid,
        hart_mask: (1 << smp) - 1,
        hart_mask_base: 0,
        delay: frequency,
    }
    .test();

    let console_result = console::test();
    pmu::test(smp);
    rfence::test(hartid, smp);
    reset::test();
    misaligned::test();
    trap::test();

    let console_passed = console_result.is_ok();
    report_console_result(console_result);
    finish(test_result && console_passed);
}

fn print_banner(hartid: usize, smp: usize, frequency: u64, dtb_pa: usize) {
    println!(
        r"
 _____         _     _  __                    _
|_   _|__  ___| |_  | |/ /___ _ __ _ __   ___| |
  | |/ _ \/ __| __| | ' // _ \ '__| '_ \ / _ \ |
  | |  __/\__ \ |_  | . \  __/ |  | | | |  __/ |
  |_|\___||___/\__| |_|\_\___|_|  |_| |_|\___|_|
================================================
| boot hart id          | {hartid:20} |
| smp                   | {smp:20} |
| timebase frequency    | {frequency:17} Hz |
| dtb physical address  | {dtb_pa:#20x} |
------------------------------------------------"
    );
}

fn report_console_result(result: console::TestResult) {
    match result {
        Ok(console::TestOutcome::Unavailable) => {
            println!("Sbi `DBCN` return contract test skipped: extension unavailable");
        }
        Ok(console::TestOutcome::Complete { inconclusive }) => {
            if inconclusive != 0 {
                println!("DBCN: {inconclusive} data-path checks inconclusive due to DENIED/FAILED");
            }
            println!("Sbi `DBCN` return contract test pass");
        }
        Err(errors) => {
            for error in errors.into_iter().flatten() {
                println!(
                    "DBCN {} FAILED: test error {} ({:?}), capacity={}, SBI error={}, value={}",
                    error.operation,
                    error.code as u8,
                    error.code,
                    error.capacity,
                    error.returned_error,
                    error.returned_value
                );
            }
        }
    }
}

fn finish(passed: bool) -> ! {
    if passed {
        println!("SBI tests completed: PASS");
        sbi::system_reset(sbi::Shutdown, sbi::NoReason);
    } else {
        println!("SBI tests completed: FAILED");
        sbi::system_reset(sbi::Shutdown, sbi::SystemFailure);
    }
    unreachable!()
}

#[cfg_attr(not(test), panic_handler)]
fn panic(info: &core::panic::PanicInfo) -> ! {
    let (hart_id, pc): (usize, usize);
    unsafe { asm!("mv    {}, tp", out(reg) hart_id) };
    unsafe { asm!("auipc {},  0", out(reg) pc) };
    println!("[test-kernel-panic] hart {hart_id} {info}");
    println!("[test-kernel-panic] pc = {pc:#x}");
    println!("[test-kernel-panic] SBI test FAILED due to panic");
    sbi::system_reset(sbi::Shutdown, sbi::SystemFailure);
    loop {}
}
