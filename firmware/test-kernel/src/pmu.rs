//! SBI PMU counter tests for the QEMU test kernel.
//!
//! The suite follows the PMU flow in three stages: enumerate counters,
//! validate event selection, then exercise one hardware and one firmware
//! counter. The firmware-counter test intentionally sends an invalid IPI so
//! that the counter's increment contract is observable.

use riscv::register::cycle;
use sbi_spec::{
    binary::{CounterMask, HartMask, SbiRet},
    pmu::{firmware_event, flags},
};
use sbi_testing::sbi::{self, ConfigFlagsParam, StartFlagsParam, StopFlagsParam};

/// Runs PMU enumeration, configuration, and counter-lifecycle checks.
pub(crate) fn test(smp: usize) {
    report_counters();
    test_start_requires_event();
    test_hardware_event_configuration();
    test_hardware_cycles();
    test_firmware_ipi_counter(smp + 1);
}

fn report_counters() {
    let counters_num = sbi::pmu_num_counters();
    for invalid_idx in [0, counters_num, usize::MAX] {
        assert_eq!(
            sbi::pmu_counter_fw_read_hi(invalid_idx),
            SbiRet::invalid_param()
        );
    }
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

fn test_start_requires_event() {
    let counter_idx = (0..sbi::pmu_num_counters())
        .find(|&idx| {
            let info = sbi::pmu_counter_get_info(idx);
            info.is_ok()
                && CounterInfo::new(info.value).is_firmware_counter()
                && sbi::pmu_counter_fw_read(idx) == SbiRet::invalid_param()
        })
        .expect("PMU tests require an unused firmware counter");
    assert_eq!(
        sbi::pmu_counter_start(
            CounterMask::from_mask_base(1, counter_idx),
            Flag::new(flags::StartFlags::INIT_VALUE.bits()),
            7,
        ),
        SbiRet::invalid_param()
    );
    assert_eq!(
        sbi::pmu_counter_fw_read(counter_idx),
        SbiRet::invalid_param()
    );
}

fn test_hardware_event_configuration() {
    // Fixed counters may still be running after the preceding SBI tests.
    // Prepare each independently so an already stopped counter cannot prevent
    // the other fixed counter from being stopped and released.
    for counter_idx in [0, 2] {
        let result = sbi::pmu_counter_stop(
            CounterMask::from_mask_base(1, counter_idx),
            Flag::new(flags::StopFlags::RESET.bits()),
        );
        assert!(result.is_ok() || result == SbiRet::already_stopped());
    }
    let counter_mask = CounterMask::from_mask_base(0x7ffff, 0);
    let flags =
        Flag::new((flags::ConfigFlags::CLEAR_VALUE | flags::ConfigFlags::AUTO_START).bits());
    for event in [0x2, 0x10019, 0x1001b, 0x10021] {
        let result = sbi::pmu_counter_config_matching(counter_mask, flags, event, 0);
        assert!(result.is_ok());
        assert!(
            sbi::pmu_counter_stop(
                CounterMask::from_mask_base(1, result.value),
                Flag::new(flags::StopFlags::RESET.bits()),
            )
            .is_ok()
        );
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
        Flag::new(flags::ConfigFlags::CLEAR_VALUE.bits()),
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
        Flag::new(flags::StartFlags::INIT_VALUE.bits()),
        0xffff,
    );
    assert!(start_result.is_ok());
    let cycle_num = read_hardware_counter(cycle_counter_csr);
    assert!(cycle_num >= 0xffff);

    let stop_result = sbi::pmu_counter_stop(
        CounterMask::from_mask_base(0x1, cycle_counter_idx),
        Flag::new(flags::StopFlags::empty().bits()),
    );
    assert!(stop_result.is_ok());
    let stopped_cycle_num = read_hardware_counter(cycle_counter_csr);
    execute_counter_workload();
    assert_eq!(read_hardware_counter(cycle_counter_csr), stopped_cycle_num);

    // Restarting with update=0 must resume from the stopped value.
    let start_result = sbi::pmu_counter_start(
        CounterMask::from_mask_base(0x1, cycle_counter_idx),
        Flag::new(flags::StartFlags::empty().bits()),
        0,
    );
    assert!(start_result.is_ok());
    execute_counter_workload();
    let restart_cycle_num = read_hardware_counter(cycle_counter_csr);
    assert!(restart_cycle_num > stopped_cycle_num);

    // An already running counter rejects SKIP_MATCH | CLEAR_VALUE | AUTO_START
    // without clearing its value before reporting the error.
    assert_eq!(
        sbi::pmu_counter_config_matching(
            CounterMask::from_mask_base(1, cycle_counter_idx),
            Flag::new(
                (flags::ConfigFlags::SKIP_MATCH
                    | flags::ConfigFlags::CLEAR_VALUE
                    | flags::ConfigFlags::AUTO_START)
                    .bits()
            ),
            0x1,
            0,
        ),
        SbiRet::not_supported()
    );
    assert!(read_hardware_counter(cycle_counter_csr) >= restart_cycle_num);

    // Configuration must also reject clearing a running counter without
    // requesting an automatic start.
    let before_config = read_hardware_counter(cycle_counter_csr);
    assert_eq!(
        sbi::pmu_counter_config_matching(
            CounterMask::from_mask_base(1, cycle_counter_idx),
            Flag::new((flags::ConfigFlags::SKIP_MATCH | flags::ConfigFlags::CLEAR_VALUE).bits()),
            0x1,
            0,
        ),
        SbiRet::not_supported()
    );
    assert!(read_hardware_counter(cycle_counter_csr) >= before_config);
    assert!(
        sbi::pmu_counter_stop(
            CounterMask::from_mask_base(1, cycle_counter_idx),
            Flag::new(flags::StopFlags::RESET.bits()),
        )
        .is_ok()
    );
    assert_eq!(
        sbi::pmu_counter_start(
            CounterMask::from_mask_base(1, cycle_counter_idx),
            Flag::new(flags::StartFlags::INIT_VALUE.bits()),
            7,
        ),
        SbiRet::invalid_param()
    );
}

fn execute_counter_workload() {
    for _ in 0..1000 {
        // Keep `pure` unset so the compiler retains this counter workload.
        // SAFETY: `nop` accesses neither memory nor stack and clobbers no registers.
        unsafe { core::arch::asm!("nop", options(nomem, nostack)) };
    }
}

fn test_firmware_ipi_counter(invalid_hart: usize) {
    // RV32's mask covers counters 0..31, including the firmware counters
    // used below; RV64 can also select counters 32..34.
    let counter_mask = CounterMask::from_mask_base(0x7_ffff_ffffu64 as usize, 0);

    // Access-fault counters start at zero and can be released for reuse.
    for event in [firmware_event::ACCESS_LOAD, firmware_event::ACCESS_STORE] {
        let result = sbi::pmu_counter_config_matching(
            counter_mask,
            Flag::new((flags::ConfigFlags::CLEAR_VALUE | flags::ConfigFlags::AUTO_START).bits()),
            EventIdx::new_firmware_event(event).raw(),
            0,
        );
        assert!(result.is_ok());
        let info = sbi::pmu_counter_get_info(result.value);
        assert!(info.is_ok() && CounterInfo::new(info.value).is_firmware_counter());
        assert_eq!(sbi::pmu_counter_fw_read(result.value), SbiRet::success(0));
        assert!(
            sbi::pmu_counter_stop(
                CounterMask::from_mask_base(1, result.value),
                Flag::new(flags::StopFlags::RESET.bits())
            )
            .is_ok()
        );
        assert_eq!(
            sbi::pmu_counter_start(
                CounterMask::from_mask_base(1, result.value),
                Flag::new(flags::StartFlags::INIT_VALUE.bits()),
                7,
            ),
            SbiRet::invalid_param()
        );
    }

    // IPI_SENT is a firmware counter and starts at zero.
    let result = sbi::pmu_counter_config_matching(
        counter_mask,
        Flag::new(flags::ConfigFlags::CLEAR_VALUE.bits()),
        EventIdx::new_firmware_event(firmware_event::IPI_SENT).raw(),
        0,
    );
    assert!(result.is_ok());
    assert!(result.value >= 19);
    let ipi_counter_idx = result.value;
    let ipi_num = sbi::pmu_counter_fw_read(ipi_counter_idx);
    assert!(ipi_num.is_ok());
    assert_eq!(ipi_num.value, 0);

    // An invalid mapping later in the mask must not start or update the
    // configured counter that comes before it.
    let unused_counter_idx = (ipi_counter_idx + 1..sbi::pmu_num_counters())
        .find(|&idx| sbi::pmu_counter_fw_read(idx) == SbiRet::invalid_param())
        .expect("PMU tests require a second unused firmware counter");
    let unused_bit = 1usize
        .checked_shl((unused_counter_idx - ipi_counter_idx) as u32)
        .expect("firmware counter pair must fit the SBI mask");
    assert_eq!(
        sbi::pmu_counter_start(
            CounterMask::from_mask_base(1 | unused_bit, ipi_counter_idx),
            Flag::new(flags::StartFlags::INIT_VALUE.bits()),
            99,
        ),
        SbiRet::invalid_param()
    );
    assert_eq!(
        sbi::pmu_counter_fw_read(ipi_counter_idx),
        SbiRet::success(0)
    );

    // Updating the counter while starting it sets the requested initial value.
    let start_result = sbi::pmu_counter_start(
        CounterMask::from_mask_base(0x1, ipi_counter_idx),
        Flag::new(flags::StartFlags::INIT_VALUE.bits()),
        25,
    );
    assert!(start_result.is_ok());
    let ipi_num = sbi::pmu_counter_fw_read(ipi_counter_idx);
    assert!(ipi_num.is_ok());
    assert_eq!(ipi_num.value, 25);
    assert_eq!(
        sbi::pmu_counter_fw_read_hi(ipi_counter_idx),
        SbiRet::success(0)
    );

    for config_flags in [
        flags::ConfigFlags::SKIP_MATCH
            | flags::ConfigFlags::CLEAR_VALUE
            | flags::ConfigFlags::AUTO_START,
        flags::ConfigFlags::SKIP_MATCH | flags::ConfigFlags::CLEAR_VALUE,
    ] {
        assert_eq!(
            sbi::pmu_counter_config_matching(
                CounterMask::from_mask_base(1, ipi_counter_idx),
                Flag::new(config_flags.bits()),
                EventIdx::new_firmware_event(firmware_event::IPI_SENT).raw(),
                0,
            ),
            SbiRet::not_supported()
        );
        assert_eq!(
            sbi::pmu_counter_fw_read(ipi_counter_idx),
            SbiRet::success(25)
        );
    }

    // IPI_SENT counts sending attempts before target validation. This rejected
    // attempt increments the active counter without delivering an interrupt.
    let send_ipi_result = sbi::send_ipi(HartMask::from_mask_base(0x1, invalid_hart));
    assert_eq!(send_ipi_result, SbiRet::invalid_param());
    let ipi_num = sbi::pmu_counter_fw_read(ipi_counter_idx);
    assert!(ipi_num.is_ok());
    assert_eq!(ipi_num.value, 26);

    let stop_result = sbi::pmu_counter_stop(
        CounterMask::from_mask_base(0x1, ipi_counter_idx),
        Flag::new(flags::StopFlags::empty().bits()),
    );
    assert!(stop_result.is_ok());
    assert_eq!(
        sbi::pmu_counter_stop(
            CounterMask::from_mask_base(0x1, ipi_counter_idx),
            Flag::new(flags::StopFlags::empty().bits()),
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
        Flag::new(flags::StartFlags::empty().bits()),
        0,
    );
    assert!(start_result.is_ok());
    let send_ipi_result = sbi::send_ipi(HartMask::from_mask_base(0x1, invalid_hart));
    assert_eq!(send_ipi_result, SbiRet::invalid_param());
    let ipi_num = sbi::pmu_counter_fw_read(ipi_counter_idx);
    assert!(ipi_num.is_ok());
    assert_eq!(ipi_num.value, 27);

    // Firmware counters wrap at 64 bits even in builds with overflow checks.
    assert!(
        sbi::pmu_counter_stop(
            CounterMask::from_mask_base(1, ipi_counter_idx),
            Flag::new(flags::StopFlags::empty().bits())
        )
        .is_ok()
    );
    assert!(
        sbi::pmu_counter_start(
            CounterMask::from_mask_base(1, ipi_counter_idx),
            Flag::new(flags::StartFlags::INIT_VALUE.bits()),
            u64::MAX,
        )
        .is_ok()
    );
    assert_eq!(
        sbi::pmu_counter_fw_read(ipi_counter_idx),
        SbiRet::success(usize::MAX)
    );
    #[cfg(target_pointer_width = "32")]
    assert_eq!(
        sbi::pmu_counter_fw_read_hi(ipi_counter_idx),
        SbiRet::success(usize::MAX)
    );
    #[cfg(target_pointer_width = "64")]
    assert_eq!(
        sbi::pmu_counter_fw_read_hi(ipi_counter_idx),
        SbiRet::success(0)
    );
    assert_eq!(
        sbi::send_ipi(HartMask::from_mask_base(1, invalid_hart)),
        SbiRet::invalid_param()
    );
    assert_eq!(
        sbi::pmu_counter_fw_read(ipi_counter_idx),
        SbiRet::success(0)
    );
    assert_eq!(
        sbi::pmu_counter_fw_read_hi(ipi_counter_idx),
        SbiRet::success(0)
    );
    assert!(
        sbi::pmu_counter_stop(
            CounterMask::from_mask_base(1, ipi_counter_idx),
            Flag::new(flags::StopFlags::RESET.bits()),
        )
        .is_ok()
    );
    assert_eq!(
        sbi::pmu_counter_start(
            CounterMask::from_mask_base(1, ipi_counter_idx),
            Flag::new(flags::StartFlags::INIT_VALUE.bits()),
            7,
        ),
        SbiRet::invalid_param()
    );
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

/// A PMU flag argument shared by configuration, start, and stop calls.
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

/// Packed counter information returned by `pmu_counter_get_info`.
struct CounterInfo {
    /// Raw encoding: bits 11:0 are the CSR number, bits 17:12 the width minus
    /// one, and the most significant bit marks a firmware counter.
    inner: usize,
}

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
    fn get_csr(&self) -> usize {
        self.inner & Self::CSR_MASK
    }

    #[inline]
    fn get_width(&self) -> usize {
        (self.inner & Self::WIDTH_MASK) >> 12
    }

    #[inline]
    fn is_firmware_counter(&self) -> bool {
        self.inner & Self::FIRMWARE_FLAG != 0
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
