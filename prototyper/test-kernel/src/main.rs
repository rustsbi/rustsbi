#![no_std]
#![no_main]
#![feature(naked_functions)]
#![allow(static_mut_refs)]

#[macro_use]
extern crate rcore_console;

use core::{
    arch::{asm, naked_asm},
    ptr::null,
};
use riscv::register::cycle;
use sbi_spec::{
    binary::{CounterMask, HartMask, SbiRet},
    pmu::firmware_event,
};
use sbi_testing::sbi::{self, ConfigFlags, StartFlags, StopFlags};
// use sbi_spec::pmu::*;
use uart16550::Uart16550;

const RISCV_HEAD_FLAGS: u64 = 0;
const RISCV_HEADER_VERSION: u32 = 0x2;
const RISCV_IMAGE_MAGIC: u64 = 0x5643534952; /* Magic number, little endian, "RISCV" */
const RISCV_IMAGE_MAGIC2: u32 = 0x05435352; /* Magic number 2, little endian, "RSC\x05" */

/// boot header
#[naked]
#[unsafe(no_mangle)]
#[unsafe(link_section = ".head.text")]
unsafe extern "C" fn _boot_header() -> ! {
    unsafe {
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
}

/// 内核入口。
///
/// # Safety
///
/// 裸函数。
#[naked]
#[unsafe(no_mangle)]
#[unsafe(link_section = ".text.entry")]
unsafe extern "C" fn _start(hartid: usize, device_tree_paddr: usize) -> ! {
    const STACK_SIZE: usize = 16384; // 16 KiB

    #[unsafe(link_section = ".bss.uninit")]
    static mut STACK: [u8; STACK_SIZE] = [0u8; STACK_SIZE];

    unsafe {
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
}

extern "C" fn rust_main(hartid: usize, dtb_pa: usize) -> ! {
    let BoardInfo {
        smp,
        frequency,
        uart,
    } = BoardInfo::parse(dtb_pa);
    unsafe { UART = Uart16550Map(uart as _) };
    rcore_console::init_console(&Console);
    rcore_console::set_log_level(option_env!("LOG"));
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
    let testing = sbi_testing::Testing {
        hartid,
        hart_mask: (1 << smp) - 1,
        hart_mask_base: 0,
        delay: frequency,
    };
    let test_result = testing.test();

    // PMU test, only available in qemu-system-riscv64 single core
    let counters_num = sbi::pmu_num_counters();
    println!("[pmu] counters number: {}", counters_num);
    for idx in 0..counters_num {
        let counter_info = sbi::pmu_counter_get_info(idx);
        let counter_info = CounterInfo::new(counter_info.value);
        if counter_info.is_firmware_counter() {
            println!("[pmu] counter index:{:>2}, is a firmware counter", idx);
        } else {
            println!(
                "[pmu] counter index:{:>2}, csr num: {:#03x}, width: {}",
                idx,
                counter_info.get_csr(),
                counter_info.get_width()
            );
        }
    }

    /* PMU test for hardware event */
    let counter_mask = CounterMask::from_mask_base(0x7ffff, 0);
    let result = sbi::pmu_counter_config_matching(counter_mask, Flag::new(0b110), 0x2, 0);
    assert!(result.is_ok());
    let result = sbi::pmu_counter_config_matching(counter_mask, Flag::new(0b110), 0x10019, 0);
    assert!(result.is_ok());
    let result = sbi::pmu_counter_config_matching(counter_mask, Flag::new(0b110), 0x1001b, 0);
    assert!(result.is_ok());
    let result = sbi::pmu_counter_config_matching(counter_mask, Flag::new(0b110), 0x10021, 0);
    assert!(result.is_ok());
    let result = sbi::pmu_counter_config_matching(counter_mask, Flag::new(0b110), 0x3, 0);
    assert_eq!(result, SbiRet::not_supported());

    // `SBI_PMU_HW_CPU_CYCLES` event test
    let result = sbi::pmu_counter_config_matching(counter_mask, Flag::new(0b010), 0x1, 0);
    assert!(result.is_ok());
    // the counter index should be 0(mcycle)
    assert_eq!(result.value, 0);
    let cycle_counter_idx = result.value;
    let cycle_num = cycle::read64();
    assert_eq!(cycle_num, 0);
    // Start counting `SBI_PMU_HW_CPU_CYCLES` events
    let start_result = sbi::pmu_counter_start(
        CounterMask::from_mask_base(0x1, cycle_counter_idx),
        Flag::new(0x1),
        0xffff,
    );
    assert!(start_result.is_ok());
    let cycle_num = cycle::read64();
    assert!(cycle_num >= 0xffff);
    // Stop counting `SBI_PMU_HW_CPU_CYCLES` events
    let stop_result = sbi::pmu_counter_stop(
        CounterMask::from_mask_base(0x1, cycle_counter_idx),
        Flag::new(0x0),
    );
    assert!(stop_result.is_ok());
    let old_cycle_num = cycle::read64();
    let mut _j = 0;
    for i in 0..1000 {
        _j += i
    }
    let new_cycle_num = cycle::read64();
    assert_eq!(old_cycle_num, new_cycle_num);
    // Restart counting `SBI_PMU_HW_CPU_CYCLES` events
    let start_result = sbi::pmu_counter_start(
        CounterMask::from_mask_base(0x1, cycle_counter_idx),
        Flag::new(0x0),
        0,
    );
    assert!(start_result.is_ok());
    let mut _j = 0;
    for i in 0..1000 {
        _j += i
    }
    let restart_cycle_num = cycle::read64();
    assert!(restart_cycle_num > new_cycle_num);

    /* PMU test for firmware  event */
    let counter_mask = CounterMask::from_mask_base(0x7ffffffff, 0);

    // Mapping a counter to the `SBI_PMU_FW_ACCESS_LOAD` event should result in unsupported
    let result = sbi::pmu_counter_config_matching(
        counter_mask,
        Flag::new(0b010),
        EventIdx::new_firmware_event(firmware_event::ACCESS_LOAD).raw(),
        0,
    );
    assert_eq!(result, SbiRet::not_supported());

    // Map a counter to the `SBI_PMU_FW_IPI_SENT` event.
    // This counter should be a firmware counter and its value should be initialized to 0.
    let result = sbi::pmu_counter_config_matching(
        counter_mask,
        Flag::new(0b010),
        EventIdx::new_firmware_event(firmware_event::IPI_SENT).raw(),
        0,
    );
    assert!(result.is_ok());
    assert!(result.value >= 19);
    let ipi_counter_idx = result.value;
    let ipi_num = sbi::pmu_counter_fw_read(ipi_counter_idx);
    assert!(ipi_num.is_ok());
    assert_eq!(ipi_num.value, 0);

    // Start counting `SBI_PMU_FW_IPI_SENT` events and assign an initial value of 25 to the event counter
    let start_result = sbi::pmu_counter_start(
        CounterMask::from_mask_base(0x1, ipi_counter_idx),
        Flag::new(0x1),
        25,
    );
    assert!(start_result.is_ok());
    // Read the value of the `SBI_PMU_FW_IPI_SENT` event counter, which should be 25
    let ipi_num = sbi::pmu_counter_fw_read(ipi_counter_idx);
    assert!(ipi_num.is_ok());
    assert_eq!(ipi_num.value, 25);

    // Send IPI to other core, and the `SBI_PMU_FW_IPI_SENT` event counter value increases by one
    let send_ipi_result = sbi::send_ipi(HartMask::from_mask_base(0b10, 0));
    assert_eq!(send_ipi_result, SbiRet::invalid_param());

    // Read the value of the `SBI_PMU_FW_IPI_SENT` event counter, which should be 26
    let ipi_num = sbi::pmu_counter_fw_read(ipi_counter_idx);
    assert!(ipi_num.is_ok());
    assert_eq!(ipi_num.value, 26);

    // Stop counting `SBI_PMU_FW_IPI_SENT` events
    let stop_result = sbi::pmu_counter_stop(
        CounterMask::from_mask_base(0x1, ipi_counter_idx),
        Flag::new(0x0),
    );
    assert!(stop_result.is_ok());

    // Restop counting `SBI_PMU_FW_IPI_SENT` events, the result should be already stop
    let stop_result = sbi::pmu_counter_stop(
        CounterMask::from_mask_base(0x1, ipi_counter_idx),
        Flag::new(0x0),
    );
    assert_eq!(stop_result, SbiRet::already_stopped());

    // Send IPI to other core, `SBI_PMU_FW_IPI_SENT` event counter should not change
    let send_ipi_result = sbi::send_ipi(HartMask::from_mask_base(0b10, 0));
    assert_eq!(send_ipi_result, SbiRet::invalid_param());

    // Read the value of the `SBI_PMU_FW_IPI_SENT` event counter, which should be 26
    let ipi_num = sbi::pmu_counter_fw_read(ipi_counter_idx);
    assert!(ipi_num.is_ok());
    assert_eq!(ipi_num.value, 26);

    // Restart counting `SBI_PMU_FW_IPI_SENT` events
    let start_result = sbi::pmu_counter_start(
        CounterMask::from_mask_base(0x1, ipi_counter_idx),
        Flag::new(0x0),
        0,
    );
    assert!(start_result.is_ok());

    // Send IPI to other core, and the `SBI_PMU_FW_IPI_SENT` event counter value increases by one
    let send_ipi_result = sbi::send_ipi(HartMask::from_mask_base(0b10, 0));
    assert_eq!(send_ipi_result, SbiRet::invalid_param());

    // Read the value of the `SBI_PMU_FW_IPI_SENT` event counter, which should be 27
    let ipi_num = sbi::pmu_counter_fw_read(ipi_counter_idx);
    assert!(ipi_num.is_ok());
    assert_eq!(ipi_num.value, 27);

    if test_result {
        sbi::system_reset(sbi::Shutdown, sbi::NoReason);
    } else {
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

struct BoardInfo {
    smp: usize,
    frequency: u64,
    uart: usize,
}

impl BoardInfo {
    fn parse(dtb_pa: usize) -> Self {
        use dtb_walker::{Dtb, DtbObj, HeaderError as E, Property, Str, WalkOperation::*};

        let mut ans = Self {
            smp: 0,
            frequency: 0,
            uart: 0,
        };
        unsafe {
            Dtb::from_raw_parts_filtered(dtb_pa as _, |e| {
                matches!(e, E::Misaligned(4) | E::LastCompVersion(_))
            })
        }
        .unwrap()
        .walk(|ctx, obj| match obj {
            DtbObj::SubNode { name } => {
                if ctx.is_root() && (name == Str::from("cpus") || name == Str::from("soc")) {
                    StepInto
                } else if ctx.name() == Str::from("cpus") && name.starts_with("cpu@") {
                    ans.smp += 1;
                    StepOver
                } else if ctx.name() == Str::from("soc")
                    && (name.starts_with("uart") || name.starts_with("serial"))
                {
                    StepInto
                } else {
                    StepOver
                }
            }
            DtbObj::Property(Property::Reg(mut reg)) => {
                if ctx.name().starts_with("uart") || ctx.name().starts_with("serial") {
                    ans.uart = reg.next().unwrap().start;
                }
                StepOut
            }
            DtbObj::Property(Property::General { name, value }) => {
                if ctx.name() == Str::from("cpus") && name == Str::from("timebase-frequency") {
                    ans.frequency = match *value {
                        [a, b, c, d] => u32::from_be_bytes([a, b, c, d]) as _,
                        [a, b, c, d, e, f, g, h] => u64::from_be_bytes([a, b, c, d, e, f, g, h]),
                        _ => unreachable!(),
                    };
                }
                StepOver
            }
            DtbObj::Property(_) => StepOver,
        });
        ans
    }
}

struct Console;
static mut UART: Uart16550Map = Uart16550Map(null());

pub struct Uart16550Map(*const Uart16550<u8>);

unsafe impl Sync for Uart16550Map {}

impl Uart16550Map {
    #[inline]
    pub fn get(&self) -> &Uart16550<u8> {
        unsafe { &*self.0 }
    }
}

impl rcore_console::Console for Console {
    #[inline]
    fn put_char(&self, c: u8) {
        unsafe { UART.get().write(core::slice::from_ref(&c)) };
    }

    #[inline]
    fn put_str(&self, s: &str) {
        unsafe { UART.get().write(s.as_bytes()) };
    }
}

struct Flag {
    inner: usize,
}

impl ConfigFlags for Flag {
    fn raw(&self) -> usize {
        self.inner
    }
}

impl StartFlags for Flag {
    fn raw(&self) -> usize {
        self.inner
    }
}

impl StopFlags for Flag {
    fn raw(&self) -> usize {
        self.inner
    }
}

impl Flag {
    pub fn new(flag: usize) -> Self {
        Self { inner: flag }
    }
}

/// Wrap for counter info
struct CounterInfo {
    /// Packed representation of counter information:
    /// - Bits [11:0]: CSR number for hardware counters
    /// - Bits [17:12]: Counter width (typically 63 for RV64)
    /// - MSB: Set for firmware counters, clear for hardware counters
    inner: usize,
}

#[allow(unused)]
impl CounterInfo {
    const CSR_MASK: usize = 0xFFF; // Bits [11:0]
    const WIDTH_MASK: usize = 0x3F << 12; // Bits [17:12]
    const FIRMWARE_FLAG: usize = 1 << (size_of::<usize>() * 8 - 1); // MSB

    #[inline]
    pub const fn new(counter_info: usize) -> Self {
        Self {
            inner: counter_info,
        }
    }

    #[inline]
    pub fn set_csr(&mut self, csr_num: u16) {
        self.inner = (self.inner & !Self::CSR_MASK) | ((csr_num as usize) & Self::CSR_MASK);
    }

    #[inline]
    pub fn get_csr(&self) -> usize {
        self.inner & Self::CSR_MASK
    }

    #[inline]
    pub fn set_width(&mut self, width: u8) {
        self.inner = (self.inner & !Self::WIDTH_MASK) | (((width as usize) & 0x3F) << 12);
    }

    #[inline]
    pub fn get_width(&self) -> usize {
        (self.inner & Self::WIDTH_MASK) >> 12
    }

    #[inline]
    pub fn is_firmware_counter(&self) -> bool {
        self.inner & Self::FIRMWARE_FLAG != 0
    }

    #[inline]
    pub const fn with_hardware_info(csr_num: u16, width: u8) -> Self {
        Self {
            inner: ((csr_num as usize) & Self::CSR_MASK) | (((width as usize) & 0x3F) << 12),
        }
    }

    #[inline]
    pub const fn with_firmware_info() -> Self {
        Self {
            inner: Self::FIRMWARE_FLAG,
        }
    }

    #[inline]
    pub const fn inner(self) -> usize {
        self.inner
    }
}

struct EventIdx {
    inner: usize,
}

impl EventIdx {
    fn raw(&self) -> usize {
        self.inner
    }

    fn new_firmware_event(event_code: usize) -> Self {
        let inner = 0xf << 16 | event_code;
        Self { inner }
    }
}
