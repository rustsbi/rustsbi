//! SBI PMU conformance checks used by the QEMU test kernel.
//!
//! The suite follows the PMU flow in three stages: enumerate counters,
//! validate event selection, then exercise one hardware and one firmware
//! counter.  The firmware-counter test intentionally sends an invalid IPI so
//! that the counter's increment contract is observable.

use riscv::register::cycle;
use sbi_spec::{
    binary::{CounterMask, HartMask, SbiRet},
    pmu::firmware_event,
};
use sbi_testing::sbi::{self, ConfigFlagsParam, StartFlagsParam, StopFlagsParam};

/// Runs the PMU checks in the order used by the expected-output script.
pub(crate) fn test(smp: usize) {
    report_counters();
    test_hardware_event_configuration();
    test_hardware_cycles();
    test_firmware_ipi_counter(smp + 1);
}

fn report_counters() {
    let counters_num = sbi::pmu_num_counters();
    println!("[pmu] counters number: {}", counters_num);
    for idx in 0..counters_num {
        let counter_info = CounterInfo::new(sbi::pmu_counter_get_info(idx).value);
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
}

fn test_hardware_event_configuration() {
    let counter_mask = CounterMask::from_mask_base(0x7ffff, 0);
    let flags = Flag::new(0b110);
    for event in [0x2, 0x10019, 0x1001b, 0x10021] {
        assert!(sbi::pmu_counter_config_matching(counter_mask, flags, event, 0).is_ok());
    }
    assert_eq!(
        sbi::pmu_counter_config_matching(counter_mask, flags, 0x3, 0),
        SbiRet::not_supported()
    );
}

fn test_hardware_cycles() {
    // `SBI_PMU_HW_CPU_CYCLES` event.
    let result = sbi::pmu_counter_config_matching(
        CounterMask::from_mask_base(0x7ffff, 0),
        Flag::new(0b010),
        0x1,
        0,
    );
    assert!(result.is_ok());
    let cycle_counter_idx = result.value;

    let counter_info = sbi::pmu_counter_get_info(cycle_counter_idx);
    assert!(counter_info.is_ok());
    let counter_info = CounterInfo::new(counter_info.value);
    assert!(!counter_info.is_firmware_counter());
    let cycle_counter_csr = counter_info.get_csr();

    // Start with a non-zero value and verify the hardware counter advances.
    let start_result = sbi::pmu_counter_start(
        CounterMask::from_mask_base(0x1, cycle_counter_idx),
        Flag::new(0x1),
        0xffff,
    );
    assert!(start_result.is_ok());
    let cycle_num = read_hardware_counter(cycle_counter_csr);
    assert!(cycle_num >= 0xffff);

    let stop_result = sbi::pmu_counter_stop(
        CounterMask::from_mask_base(0x1, cycle_counter_idx),
        Flag::new(0x0),
    );
    assert!(stop_result.is_ok());
    let mut _sum = 0;
    for i in 0..1000 {
        _sum += i;
    }
    let stopped_cycle_num = read_hardware_counter(cycle_counter_csr);

    // Restarting with update=0 must resume from the stopped value.
    let start_result = sbi::pmu_counter_start(
        CounterMask::from_mask_base(0x1, cycle_counter_idx),
        Flag::new(0x0),
        0,
    );
    assert!(start_result.is_ok());
    let mut _sum = 0;
    for i in 0..1000 {
        _sum += i;
    }
    let restart_cycle_num = read_hardware_counter(cycle_counter_csr);
    assert!(restart_cycle_num > stopped_cycle_num);
}

fn test_firmware_ipi_counter(invalid_hart: usize) {
    // RV32's mask covers counters 0..31, including the firmware counters
    // used below; RV64 can also select counters 32..34.
    let counter_mask = CounterMask::from_mask_base(0x7_ffff_ffffu64 as usize, 0);

    // Access-fault counters start at zero and can be released for reuse.
    for event in [firmware_event::ACCESS_LOAD, firmware_event::ACCESS_STORE] {
        let result = sbi::pmu_counter_config_matching(
            counter_mask,
            Flag::new(0b110),
            EventIdx::new_firmware_event(event).raw(),
            0,
        );
        assert!(result.is_ok());
        let info = sbi::pmu_counter_get_info(result.value);
        assert!(info.is_ok() && CounterInfo::new(info.value).is_firmware_counter());
        assert_eq!(sbi::pmu_counter_fw_read(result.value), SbiRet::success(0));
        assert!(
            sbi::pmu_counter_stop(CounterMask::from_mask_base(1, result.value), Flag::new(1))
                .is_ok()
        );
    }

    // IPI_SENT is a firmware counter and starts at zero.
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

    // Updating the counter while starting it sets the requested initial value.
    let start_result = sbi::pmu_counter_start(
        CounterMask::from_mask_base(0x1, ipi_counter_idx),
        Flag::new(0x1),
        25,
    );
    assert!(start_result.is_ok());
    let ipi_num = sbi::pmu_counter_fw_read(ipi_counter_idx);
    assert!(ipi_num.is_ok());
    assert_eq!(ipi_num.value, 25);

    // A rejected IPI must not be sent or counted.
    let send_ipi_result = sbi::send_ipi(HartMask::from_mask_base(0x1, invalid_hart));
    assert_eq!(send_ipi_result, SbiRet::invalid_param());
    let ipi_num = sbi::pmu_counter_fw_read(ipi_counter_idx);
    assert!(ipi_num.is_ok());
    assert_eq!(ipi_num.value, 26);

    let stop_result = sbi::pmu_counter_stop(
        CounterMask::from_mask_base(0x1, ipi_counter_idx),
        Flag::new(0x0),
    );
    assert!(stop_result.is_ok());
    assert_eq!(
        sbi::pmu_counter_stop(
            CounterMask::from_mask_base(0x1, ipi_counter_idx),
            Flag::new(0x0),
        ),
        SbiRet::already_stopped()
    );

    // Invalid targets remain invisible while the firmware counter is stopped.
    let send_ipi_result = sbi::send_ipi(HartMask::from_mask_base(0x1, invalid_hart));
    assert_eq!(send_ipi_result, SbiRet::invalid_param());
    let ipi_num = sbi::pmu_counter_fw_read(ipi_counter_idx);
    assert!(ipi_num.is_ok());
    assert_eq!(ipi_num.value, 26);

    // Restart without updating the value, then observe the next rejected IPI.
    let start_result = sbi::pmu_counter_start(
        CounterMask::from_mask_base(0x1, ipi_counter_idx),
        Flag::new(0x0),
        0,
    );
    assert!(start_result.is_ok());
    let send_ipi_result = sbi::send_ipi(HartMask::from_mask_base(0x1, invalid_hart));
    assert_eq!(send_ipi_result, SbiRet::invalid_param());
    let ipi_num = sbi::pmu_counter_fw_read(ipi_counter_idx);
    assert!(ipi_num.is_ok());
    assert_eq!(ipi_num.value, 27);
}

#[inline]
fn read_hardware_counter(csr_num: usize) -> u64 {
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

/// The PMU flag parameter is shared by config, start, and stop calls.
#[derive(Clone, Copy)]
struct Flag {
    inner: usize,
}

impl Flag {
    const fn new(flag: usize) -> Self {
        Self { inner: flag }
    }
}

impl ConfigFlagsParam for Flag {
    fn raw(&self) -> usize {
        self.inner
    }
}

impl StartFlagsParam for Flag {
    fn raw(&self) -> usize {
        self.inner
    }
}

impl StopFlagsParam for Flag {
    fn raw(&self) -> usize {
        self.inner
    }
}

/// Decodes the packed value returned by `pmu_counter_get_info`.
struct CounterInfo {
    /// Bits [11:0] hold the CSR number, [17:12] the width, and the MSB marks
    /// a firmware counter.
    inner: usize,
}

#[allow(unused)]
impl CounterInfo {
    const CSR_MASK: usize = 0xFFF;
    const WIDTH_MASK: usize = 0x3F << 12;
    const FIRMWARE_FLAG: usize = 1 << (usize::BITS as usize - 1);

    #[inline]
    const fn new(counter_info: usize) -> Self {
        Self {
            inner: counter_info,
        }
    }

    #[inline]
    fn set_csr(&mut self, csr_num: u16) {
        self.inner = (self.inner & !Self::CSR_MASK) | ((csr_num as usize) & Self::CSR_MASK);
    }

    #[inline]
    fn get_csr(&self) -> usize {
        self.inner & Self::CSR_MASK
    }

    #[inline]
    fn set_width(&mut self, width: u8) {
        self.inner = (self.inner & !Self::WIDTH_MASK) | (((width as usize) & 0x3F) << 12);
    }

    #[inline]
    fn get_width(&self) -> usize {
        (self.inner & Self::WIDTH_MASK) >> 12
    }

    #[inline]
    fn is_firmware_counter(&self) -> bool {
        self.inner & Self::FIRMWARE_FLAG != 0
    }

    #[inline]
    const fn with_hardware_info(csr_num: u16, width: u8) -> Self {
        Self {
            inner: ((csr_num as usize) & Self::CSR_MASK) | (((width as usize) & 0x3F) << 12),
        }
    }

    #[inline]
    const fn with_firmware_info() -> Self {
        Self {
            inner: Self::FIRMWARE_FLAG,
        }
    }

    #[inline]
    const fn inner(self) -> usize {
        self.inner
    }
}

struct EventIdx {
    inner: usize,
}

impl EventIdx {
    const fn new_firmware_event(event_code: usize) -> Self {
        Self {
            inner: 0xf << 16 | event_code,
        }
    }

    const fn raw(&self) -> usize {
        self.inner
    }
}
