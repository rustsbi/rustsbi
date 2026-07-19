#![no_std]
#![no_main]
#![allow(static_mut_refs)]

#[macro_use]
extern crate rcore_console;

use core::{
    arch::{asm, naked_asm},
    fmt::{self, Write},
};
use rcore_console::Console as _;
use riscv::register::{cycle, satp, sie, sstatus, time};
use sbi_spec::{
    binary::{CounterMask, HartMask, Physical, SbiRet},
    pmu::firmware_event,
};
use sbi_testing::{
    protocol::{self, Run},
    sbi::{self, ConfigFlags, StartFlags, StopFlags},
};
// use sbi_spec::pmu::*;

const RISCV_HEAD_FLAGS: u64 = 0;
const RISCV_HEADER_VERSION: u32 = 0x2;
const RISCV_IMAGE_MAGIC: u64 = 0x5643534952; /* Magic number, little endian, "RISCV" */
const RISCV_IMAGE_MAGIC2: u32 = 0x05435352; /* Magic number 2, little endian, "RSC\x05" */
const SYSTEM_SUSPEND_OPAQUE: usize = 0x5355_5350;
const SYSTEM_RESUME_STACK_SIZE: usize = 16 * 1024;
const TEST_SHARD: &str = match option_env!("RUSTSBI_TEST_SHARD") {
    Some(value) => value,
    None => "s-mode-local",
};
const TEST_RUN: &str = match option_env!("RUSTSBI_TEST_RUN_ID") {
    Some(value) => value,
    None => "local",
};
const TEST_DIGEST: &str = "74106ac03c66d91e29f11059cd5aeed52607d8dcad6c57ea432be48d9571630c";
const TEST_CASES: usize = 4;

const fn test_run() -> Run<'static> {
    Run {
        shard: TEST_SHARD,
        run: TEST_RUN,
        attempt: 1,
        seed: 0,
        digest: TEST_DIGEST,
    }
}

struct ProtocolOutput;

impl Write for ProtocolOutput {
    fn write_str(&mut self, value: &str) -> fmt::Result {
        Console.put_str(value);
        Ok(())
    }
}

fn protocol_record(write: impl FnOnce(&mut ProtocolOutput) -> fmt::Result) {
    let _ = write(&mut ProtocolOutput);
}

struct InvalidSleepType;

impl sbi::SleepType for InvalidSleepType {
    fn raw(&self) -> u32 {
        1
    }
}

#[unsafe(link_section = ".bss.uninit")]
static mut SYSTEM_RESUME_STACK: [u8; SYSTEM_RESUME_STACK_SIZE] = [0; SYSTEM_RESUME_STACK_SIZE];

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

macro_rules! define_start {
    ($store:literal, $stride:literal) => {
        #[unsafe(naked)]
        #[unsafe(no_mangle)]
        #[unsafe(link_section = ".text.entry")]
        unsafe extern "C" fn _start(hartid: usize, device_tree_paddr: usize) -> ! {
            const STACK_SIZE: usize = 16384; // 16 KiB

            #[unsafe(link_section = ".bss.uninit")]
            static mut STACK: [u8; STACK_SIZE] = [0u8; STACK_SIZE];

            naked_asm!(
                // Clear BSS with one native-width store per iteration.
                concat!(
                    "   la      t0, sbss
            la      t1, ebss
        1:  bgeu    t0, t1, 2f
            ",
                    $store,
                    " zero, 0(t0)
            addi    t0, t0, ",
                    $stride,
                    "
            j       1b",
                ),
                "2:",
                "   la sp, {stack} + {stack_size}",
                "   j  {main}",
                stack_size = const STACK_SIZE,
                stack      =   sym STACK,
                main       =   sym rust_main,
            )
        }
    };
}

#[cfg(target_pointer_width = "32")]
define_start!("sw", "4");

#[cfg(target_pointer_width = "64")]
define_start!("sd", "8");

extern "C" fn rust_main(hartid: usize, dtb_pa: usize) -> ! {
    let BoardInfo { smp, frequency } = BoardInfo::parse(dtb_pa);
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
    protocol_record(|output| protocol::run_start(output, test_run(), TEST_CASES));
    protocol_record(|output| protocol::case_start(output, test_run(), "sbi.core"));
    let test_result = testing.test();
    if !test_result {
        protocol_record(|output| protocol::case_fail(output, test_run(), "sbi.core", "SBI_CORE"));
        protocol_record(|output| protocol::run_fail(output, test_run(), 0, 1, "SBI_CORE"));
        sbi::system_reset(sbi::Shutdown, sbi::SystemFailure);
    }
    protocol_record(|output| protocol::case_pass(output, test_run(), "sbi.core"));

    protocol_record(|output| protocol::case_start(output, test_run(), "sbi.pmu"));
    pmu_test(smp);
    protocol_record(|output| protocol::case_pass(output, test_run(), "sbi.pmu"));

    protocol_record(|output| protocol::case_start(output, test_run(), "sbi.rfence"));
    fence_test(hartid, smp);
    protocol_record(|output| protocol::case_pass(output, test_run(), "sbi.rfence"));

    protocol_record(|output| protocol::case_start(output, test_run(), "sbi.susp"));
    system_suspend_test(frequency);
    unreachable!()
}

fn system_suspend_test(delay: u64) {
    assert!(sbi::probe_extension(sbi::Suspend).is_available());
    assert_eq!(
        sbi::system_suspend(InvalidSleepType, system_resume as *const () as usize, 0),
        SbiRet::invalid_param()
    );

    sbi::set_timer(time::read64() + delay);
    unsafe { sie::set_stimer() };
    let ret = sbi::system_suspend(
        sbi::SuspendToRam,
        system_resume as *const () as usize,
        SYSTEM_SUSPEND_OPAQUE,
    );
    panic!("system suspend returned instead of resuming: {ret:?}")
}

#[unsafe(naked)]
unsafe extern "C" fn system_resume(hartid: usize, opaque: usize) -> ! {
    naked_asm!(
        "la sp, {stack} + {stack_size}",
        "j {main}",
        stack = sym SYSTEM_RESUME_STACK,
        stack_size = const SYSTEM_RESUME_STACK_SIZE,
        main = sym system_resume_main,
    )
}

extern "C" fn system_resume_main(hartid: usize, opaque: usize) -> ! {
    sbi::set_timer(u64::MAX);
    assert_eq!(opaque, SYSTEM_SUSPEND_OPAQUE);
    assert_eq!(satp::read().bits(), 0);
    assert!(!sstatus::read().sie());
    println!("[susp] resumed hart {hartid} with the required entry state");
    protocol_record(|output| protocol::case_pass(output, test_run(), "sbi.susp"));
    protocol_record(|output| protocol::run_pass(output, test_run(), TEST_CASES));
    sbi::system_reset(sbi::Shutdown, sbi::NoReason);
    unreachable!()
}

#[inline]
// PMU test, only available in qemu-system-riscv64 single core
fn pmu_test(smp: usize) {
    let invalid_hart = smp + 1;
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
    assert!(result.is_ok());

    // `SBI_PMU_HW_CPU_CYCLES` event test
    let result = sbi::pmu_counter_config_matching(counter_mask, Flag::new(0b010), 0x1, 0);
    assert!(result.is_ok());
    let cycle_counter_idx = result.value;
    let counter_info = sbi::pmu_counter_get_info(cycle_counter_idx);
    assert!(counter_info.is_ok());
    let counter_info = CounterInfo::new(counter_info.value);
    assert!(!counter_info.is_firmware_counter());
    let cycle_counter_csr = counter_info.get_csr();
    // Start counting `SBI_PMU_HW_CPU_CYCLES` events
    let start_result = sbi::pmu_counter_start(
        CounterMask::from_mask_base(0x1, cycle_counter_idx),
        Flag::new(0x1),
        0xffff,
    );
    assert!(start_result.is_ok());
    let cycle_num = read_pmu_hardware_counter(cycle_counter_csr);
    assert!(cycle_num >= 0xffff);
    // Stop counting `SBI_PMU_HW_CPU_CYCLES` events
    let stop_result = sbi::pmu_counter_stop(
        CounterMask::from_mask_base(0x1, cycle_counter_idx),
        Flag::new(0x0),
    );
    assert!(stop_result.is_ok());
    let mut _j = 0;
    for i in 0..1000 {
        _j += i
    }
    let stopped_cycle_num = read_pmu_hardware_counter(cycle_counter_csr);
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
    let restart_cycle_num = read_pmu_hardware_counter(cycle_counter_csr);
    assert!(restart_cycle_num > stopped_cycle_num);

    /* PMU test for firmware  event */
    let complete_mask = 1usize
        .checked_shl(counters_num as u32)
        .map_or(usize::MAX, |end| end - 1);
    let counter_mask = CounterMask::from_mask_base(complete_mask, 0);

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
    let ipi_counter_idx = result.value;
    let counter_info = sbi::pmu_counter_get_info(ipi_counter_idx);
    assert!(counter_info.is_ok());
    assert!(CounterInfo::new(counter_info.value).is_firmware_counter());
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

    // Send IPI to an invalid hart (beyond SMP count) - should return invalid_param
    let send_ipi_result = sbi::send_ipi(HartMask::from_mask_base(0x1, invalid_hart));
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

    // Send IPI to invalid hart, `SBI_PMU_FW_IPI_SENT` event counter should not change
    let send_ipi_result = sbi::send_ipi(HartMask::from_mask_base(0x1, invalid_hart));
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

    // Send IPI to an invalid hart (beyond SMP count) - should return invalid_param
    let send_ipi_result = sbi::send_ipi(HartMask::from_mask_base(0x1, invalid_hart));
    assert_eq!(send_ipi_result, SbiRet::invalid_param());

    // Read the value of the `SBI_PMU_FW_IPI_SENT` event counter, which should be 27
    let ipi_num = sbi::pmu_counter_fw_read(ipi_counter_idx);
    assert!(ipi_num.is_ok());
    assert_eq!(ipi_num.value, 27);
}

#[inline]
fn read_pmu_hardware_counter(csr_num: usize) -> u64 {
    match csr_num {
        0xc00 => cycle::read64(),
        0xc01 => riscv::register::time::read64(),
        0xc02 => riscv::register::instret::read64(),
        0xc03 => riscv::register::hpmcounter3::read64(),
        0xc04 => riscv::register::hpmcounter4::read64(),
        0xc05 => riscv::register::hpmcounter5::read64(),
        0xc06 => riscv::register::hpmcounter6::read64(),
        0xc07 => riscv::register::hpmcounter7::read64(),
        0xc08 => riscv::register::hpmcounter8::read64(),
        0xc09 => riscv::register::hpmcounter9::read64(),
        0xc0a => riscv::register::hpmcounter10::read64(),
        0xc0b => riscv::register::hpmcounter11::read64(),
        0xc0c => riscv::register::hpmcounter12::read64(),
        0xc0d => riscv::register::hpmcounter13::read64(),
        0xc0e => riscv::register::hpmcounter14::read64(),
        0xc0f => riscv::register::hpmcounter15::read64(),
        0xc10 => riscv::register::hpmcounter16::read64(),
        0xc11 => riscv::register::hpmcounter17::read64(),
        0xc12 => riscv::register::hpmcounter18::read64(),
        0xc13 => riscv::register::hpmcounter19::read64(),
        0xc14 => riscv::register::hpmcounter20::read64(),
        0xc15 => riscv::register::hpmcounter21::read64(),
        0xc16 => riscv::register::hpmcounter22::read64(),
        0xc17 => riscv::register::hpmcounter23::read64(),
        0xc18 => riscv::register::hpmcounter24::read64(),
        0xc19 => riscv::register::hpmcounter25::read64(),
        0xc1a => riscv::register::hpmcounter26::read64(),
        0xc1b => riscv::register::hpmcounter27::read64(),
        0xc1c => riscv::register::hpmcounter28::read64(),
        0xc1d => riscv::register::hpmcounter29::read64(),
        0xc1e => riscv::register::hpmcounter30::read64(),
        0xc1f => riscv::register::hpmcounter31::read64(),
        _ => panic!("unsupported hardware counter CSR {csr_num:#x}"),
    }
}

#[inline]
// Fence and HFence test
fn fence_test(hartid: usize, smp: usize) {
    let self_mask = HartMask::from_mask_base(0x1, hartid);

    // Fence.i test (should succeed, no-op for PMU, but should not panic)
    let ret = sbi::remote_fence_i(self_mask);
    assert!(ret.is_ok() || ret == SbiRet::not_supported());

    let invalid_hart = smp + 1;
    let ret = sbi::remote_fence_i(HartMask::from_mask_base(0x1, invalid_hart));
    assert_eq!(ret, SbiRet::invalid_param());

    // SFence.vma test (should succeed, no-op for PMU, but should not panic)
    let ret = sbi::remote_sfence_vma(self_mask, 0, 0);
    assert!(ret.is_ok() || ret == SbiRet::not_supported());

    let ret = sbi::remote_sfence_vma(HartMask::from_mask_base(0x1, invalid_hart), 0, 0);
    assert_eq!(ret, SbiRet::invalid_param());

    // SFence.vma with asid test
    let ret = sbi::remote_sfence_vma_asid(self_mask, 0, 0, 0);
    assert!(ret.is_ok() || ret == SbiRet::not_supported());

    // HFence tests (if supported)
    let hfence = [
        sbi::remote_hfence_gvma(self_mask, 0, 0),
        sbi::remote_hfence_gvma_vmid(self_mask, 0, 0, 0),
        sbi::remote_hfence_vvma(self_mask, 0, 0),
        sbi::remote_hfence_vvma_asid(self_mask, 0, 0, 0),
    ];
    let supported = hfence.iter().all(SbiRet::is_ok);
    let unavailable = hfence
        .iter()
        .all(|result| *result == SbiRet::not_supported());
    assert!(supported || unavailable);
    if supported {
        println!("[fence] HFENCE operations pass");
    } else {
        println!("[fence] HFENCE operations are not supported");
    }

    println!("[34m[ INFO] Sbi `RFNC` test pass[0m");
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
}

impl BoardInfo {
    fn parse(dtb_pa: usize) -> Self {
        use dtb_walker::{Dtb, DtbObj, HeaderError as E, Property, Str, WalkOperation::*};

        let mut ans = Self {
            smp: 0,
            frequency: 0,
        };
        unsafe {
            Dtb::from_raw_parts_filtered(dtb_pa as _, |e| {
                matches!(e, E::Misaligned(4) | E::LastCompVersion(_))
            })
        }
        .unwrap()
        .walk(|ctx, obj| match obj {
            DtbObj::SubNode { name } => {
                if ctx.is_root() && name == Str::from("cpus") {
                    StepInto
                } else if ctx.name() == Str::from("cpus") && name.starts_with("cpu@") {
                    ans.smp += 1;
                    StepOver
                } else {
                    StepOver
                }
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

impl rcore_console::Console for Console {
    #[inline]
    fn put_char(&self, c: u8) {
        let _ = sbi::console_write_byte(c);
    }

    #[inline]
    fn put_str(&self, mut value: &str) {
        while !value.is_empty() {
            let bytes = Physical::new(value.len(), value.as_ptr() as usize, 0);
            let Some(written) = sbi::console_write(bytes).ok() else {
                return;
            };
            if written == 0 {
                return;
            }
            let Some(remaining) = value.get(written..) else {
                return;
            };
            value = remaining;
        }
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
    const FIRMWARE_FLAG: usize = 1 << (usize::BITS as usize - 1); // MSB

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
