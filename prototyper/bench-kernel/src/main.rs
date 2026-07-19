#![no_std]
#![no_main]
#![allow(static_mut_refs)]

#[macro_use]
extern crate rcore_console;

use core::arch::{asm, naked_asm};
use core::mem::MaybeUninit;
use core::sync::{atomic::AtomicBool, atomic::AtomicU64, atomic::Ordering};
use log::*;
use sbi::SbiRet;
use sbi_spec::binary::{HartMask, MaskError};
use sbi_spec::hsm::hart_state;
use sbi_testing::sbi;
use serde::Deserialize;
use serde_device_tree::{
    Dtb, DtbPtr,
    buildin::{Node, NodeSeq, Reg, StrSeq},
};
use spin::Once;
use uart16550::Uart16550;

const RISCV_HEAD_FLAGS: u64 = 0;
const RISCV_HEADER_VERSION: u32 = 0x2;
const RISCV_IMAGE_MAGIC: u64 = 0x5643534952; /* Magic number, little endian, "RISCV" */
const RISCV_IMAGE_MAGIC2: u32 = 0x05435352; /* Magic number 2, little endian, "RSC\x05" */

/// boot header
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

const STACK_SIZE: usize = 512 * 1024; // 512 KiB
const MAX_HART_NUM: usize = 128;

#[allow(dead_code)]
#[derive(Copy, Clone)]
#[repr(C, align(16))]
struct HartStack([u8; STACK_SIZE]);

impl HartStack {
    #[inline]
    pub const fn new() -> Self {
        HartStack([0; STACK_SIZE])
    }
}

#[unsafe(link_section = ".bss.uninit")]
static mut STACK: HartStack = HartStack::new();
#[unsafe(link_section = ".bss.uninit")]
static mut HART_STACK: [HartStack; MAX_HART_NUM] = [HartStack::new(); MAX_HART_NUM];
#[unsafe(link_section = ".bss.uninit")]
static mut IPI_SENT: [MaybeUninit<AtomicBool>; MAX_HART_NUM] =
    [const { MaybeUninit::uninit() }; MAX_HART_NUM];
#[unsafe(link_section = ".bss.uninit")]
static mut SMP_COUNT: usize = 0;
#[unsafe(link_section = ".bss.uninit")]
static mut BOOT_HART_ID: usize = 0;

fn hart_stack_top(hartid: usize) -> Option<usize> {
    let next = hartid.checked_add(1)?;
    if next > MAX_HART_NUM {
        return None;
    }
    let base = core::ptr::addr_of!(HART_STACK).cast::<HartStack>();
    // SAFETY: `next` is at most the array length, so this computes either an
    // in-array element address or the permitted one-past pointer. The result
    // is used only as a descending stack pointer and is never dereferenced at
    // the one-past address.
    Some(unsafe { base.add(next) } as usize)
}

/// 内核入口。
///
/// # Safety
///
/// 裸函数。
#[unsafe(naked)]
#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.entry")]
unsafe extern "C" fn _start(hartid: usize, device_tree_paddr: usize) -> ! {
    naked_asm!(
        // clear bss segment
        "   la      t0, sbss
            la      t1, ebss
        1:  bgeu    t0, t1, 2f
            sd      zero, 0(t0)
            addi    t0, t0, 8
            j       1b",
        "2:",
        "   la sp, {stack} + {stack_size}",
        "   j  {main}",
        stack_size = const STACK_SIZE,
        stack      =   sym STACK,
        main       =   sym rust_main,
    )
}

#[unsafe(naked)]
#[unsafe(no_mangle)]
extern "C" fn init_hart(hartid: usize, opaque: usize) {
    naked_asm!(
        "add sp, a1, zero",
        "csrw sscratch, sp",
        "call {init_main}",
        init_main = sym init_main,
    )
}

#[unsafe(naked)]
#[unsafe(no_mangle)]
extern "C" fn core_send_ipi(hartid: usize, opaque: usize) {
    naked_asm!(
        "add sp, a1, zero",
        "csrw sscratch, sp",
        "call {send_ipi}",
        send_ipi = sym send_ipi,
    )
}

extern "C" fn send_ipi(hartid: usize) -> ! {
    let stack_top = hart_stack_top(hartid).unwrap_or_else(|| {
        sbi::system_reset(sbi::Shutdown, sbi::SystemFailure);
        unreachable!()
    });
    if unsafe { !(IPI_SENT[hartid].assume_init_mut().load(Ordering::Relaxed)) } {
        unsafe {
            IPI_SENT[hartid]
                .assume_init_mut()
                .swap(true, Ordering::AcqRel);
        };
        let mut mask = Some(HartMask::from_mask_base(0, 0));
        for i in 0..unsafe { SMP_COUNT } {
            if i == unsafe { BOOT_HART_ID } {
                continue;
            }
            if let Some(ref mut mask) = mask {
                match mask.insert(i) {
                    Ok(_) => continue,
                    Err(MaskError::InvalidBit) => {
                        sbi::remote_sfence_vma(*mask, 0, 0);
                    }
                    Err(_) => unreachable!("Failed to construct mask"),
                }
            }
            mask = Some(HartMask::from_mask_base(0b1, i));
        }
        if let Some(mask) = mask {
            sbi::remote_sfence_vma(mask, 0, 0);
        }
        unsafe {
            WAIT_COUNT.fetch_sub(1, Ordering::AcqRel);
            while WAIT_COUNT.load(Ordering::Relaxed) != 0 {
                core::hint::spin_loop();
            }
        }
    } else {
        unreachable!("resend {}", hartid);
    }
    sbi::hart_suspend(
        sbi::NonRetentive,
        core_send_ipi as *const () as _,
        stack_top,
    );
    unreachable!()
}

extern "C" fn init_main(hartid: usize) -> ! {
    let stack_top = hart_stack_top(hartid).unwrap_or_else(|| {
        sbi::system_reset(sbi::Shutdown, sbi::SystemFailure);
        unreachable!()
    });
    sbi::hart_suspend(
        sbi::NonRetentive,
        core_send_ipi as *const () as _,
        stack_top,
    );
    unreachable!()
}

static mut WAIT_COUNT: AtomicU64 = AtomicU64::new(0);

const SUSPENDED: SbiRet = SbiRet::success(hart_state::SUSPENDED);

fn get_time() -> u64 {
    const CSR_TIME: u32 = 0xc01;
    let mut low_time: u64;
    unsafe {
        asm!("csrr {}, {CSR_TIME}", out(reg) low_time, CSR_TIME = const CSR_TIME);
    }

    low_time
}

extern "C" fn rust_main(hartid: usize, dtb_pa: usize) -> ! {
    #[derive(Deserialize)]
    struct Tree<'a> {
        cpus: Cpus<'a>,
        chosen: Chosen<'a>,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "kebab-case")]
    struct Cpus<'a> {
        timebase_frequency: u32,
        cpu: NodeSeq<'a>,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "kebab-case")]
    struct Chosen<'a> {
        stdout_path: StrSeq<'a>,
    }
    let dtb_ptr = DtbPtr::from_raw(dtb_pa as _).unwrap();
    let dtb = Dtb::from(dtb_ptr).share();
    let root: Node = serde_device_tree::from_raw_mut(&dtb).unwrap();
    let tree: Tree = root.deserialize();
    let stdout_path = tree.chosen.stdout_path.iter().next().unwrap();
    if let Some(address) = root
        .find(stdout_path)
        .and_then(|node| {
            let property = node.get_prop("reg")?;
            property
                .deserialize::<Reg>()
                .iter()
                .next()
                .map(|range| range.0.start)
        })
        .filter(|address| *address != 0)
    {
        UART.call_once(|| Uart16550Map(address));
        rcore_console::init_console(&Console);
        rcore_console::set_log_level(option_env!("LOG"));
    }
    let smp = tree.cpus.cpu.len();
    if smp == 0 || smp > MAX_HART_NUM || hartid >= smp {
        sbi::system_reset(sbi::Shutdown, sbi::SystemFailure);
        unreachable!()
    }
    let frequency = tree.cpus.timebase_frequency;
    info!(
        r"
 ____                  _       _  __                    _
| __ )  ___ _ __   ___| |__   | |/ /___ _ __ _ __   ___| |
|  _ \ / _ \ '_ \ / __| '_ \  | ' // _ \ '__| '_ \ / _ \ |
| |_) |  __/ | | | (__| | | | | . \  __/ |  | | | |  __/ |
|____/ \___|_| |_|\___|_| |_| |_|\_\___|_|  |_| |_|\___|_|
==========================================================
| boot hart id          | {hartid:20} |
| smp                   | {smp:20} |
| timebase frequency    | {frequency:17} Hz |
| dtb physical address  | {dtb_pa:#20x} |
----------------------------------------------------------"
    );
    unsafe {
        SMP_COUNT = smp;
        BOOT_HART_ID = hartid;
    }
    // SAFETY: `smp` was bounded by `MAX_HART_NUM`, and the boot hart performs
    // this one-time initialization before starting any secondary hart.
    for (i, ipi_sent) in unsafe { IPI_SENT.iter_mut().enumerate().take(smp) } {
        ipi_sent.write(AtomicBool::new(false));
        if i != hartid {
            let stack_top = hart_stack_top(i).expect("validated hart range has a stack");
            sbi::hart_start(i, init_hart as *const () as _, stack_top);
            while sbi::hart_get_status(i) != SUSPENDED {
                core::hint::spin_loop();
            }
        }
    }
    info!("Starting test");
    for i in 0..4 {
        info!("Test #{i} started");
        unsafe {
            for (i, ipi_sent) in IPI_SENT.iter_mut().enumerate().take(smp) {
                ipi_sent.assume_init_mut().swap(false, Ordering::AcqRel);
                if i != hartid {
                    while sbi::hart_get_status(i) != SUSPENDED {
                        core::hint::spin_loop();
                    }
                }
            }
            WAIT_COUNT.swap((smp - 1) as u64, Ordering::AcqRel);
        }
        debug!("send ipi!");
        let start_time = get_time();
        let mut mask = Some(HartMask::from_mask_base(0, 0));
        for i in 0..smp {
            if i == hartid {
                continue;
            }
            if let Some(ref mut mask) = mask {
                match mask.insert(i) {
                    Ok(_) => continue,
                    Err(MaskError::InvalidBit) => {
                        sbi::send_ipi(*mask);
                    }
                    Err(_) => unreachable!("Failed to construct mask"),
                }
            }
            mask = Some(HartMask::from_mask_base(0b1, i));
        }
        if let Some(mask) = mask {
            sbi::send_ipi(mask);
        }
        while unsafe { WAIT_COUNT.load(Ordering::Acquire) } != 0 {
            core::hint::spin_loop();
        }
        let end_time = get_time();
        println!("Test #{}: {}", i, end_time - start_time);
    }
    sbi::system_reset(sbi::Shutdown, sbi::NoReason);
    unreachable!()
}

#[cfg_attr(not(test), panic_handler)]
fn panic(info: &core::panic::PanicInfo) -> ! {
    let (hart_id, pc): (usize, usize);
    unsafe { asm!("mv    {}, tp", out(reg) hart_id) };
    unsafe { asm!("auipc {},  0", out(reg) pc) };
    info!("[test-kernel-panic] hart {hart_id} {info}");
    info!("[test-kernel-panic] pc = {pc:#x}");
    info!("[test-kernel-panic] SBI test FAILED due to panic");
    sbi::system_reset(sbi::Shutdown, sbi::SystemFailure);
    loop {}
}

struct Console;
static UART: Once<Uart16550Map> = Once::new();

pub struct Uart16550Map(usize);

// The UART mapping is immutable after DT validation. Device-level concurrent
// access is supported by the byte-wide MMIO interface used by this benchmark.
unsafe impl Sync for Uart16550Map {}

impl Uart16550Map {
    #[inline]
    pub fn get(&self) -> &Uart16550<u8> {
        // SAFETY: construction occurs only after a nonzero DT `reg` address is
        // decoded for the selected console, and the mapping lives for the
        // complete benchmark execution.
        unsafe { &*(self.0 as *const Uart16550<u8>) }
    }
}

impl rcore_console::Console for Console {
    #[inline]
    fn put_char(&self, c: u8) {
        if let Some(uart) = UART.get() {
            uart.get().write(core::slice::from_ref(&c));
        }
    }

    #[inline]
    fn put_str(&self, s: &str) {
        if let Some(uart) = UART.get() {
            uart.get().write(s.as_bytes());
        }
    }
}
