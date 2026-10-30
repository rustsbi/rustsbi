//! SBI PMU conformance checks used by the QEMU test kernel.
//!
//! The suite follows the PMU flow in three stages: enumerate counters,
//! validate event selection, then exercise hardware and firmware counters.
//! Firmware-counter checks trigger access faults and attempt IPIs to invalid harts.

use riscv::register::cycle;
use sbi_spec::{
    binary::{
        CounterMask, HartMask, RET_ERR_ALREADY_STOPPED, RET_ERR_INVALID_PARAM,
        RET_ERR_NOT_SUPPORTED, SbiRet,
    },
    pmu::{
        FirmwareEvent,
        flags::{ConfigFlags, StartFlags, StopFlags},
    },
};
use sbi_testing::sbi;

/// Runs the PMU checks in the order used by the expected-output script.
pub(crate) fn test(smp: usize) {
    if sbi::probe_extension(sbi::Pmu).is_unavailable() {
        println!("[pmu] extension unavailable, skipping");
        return;
    }
    report_counters();
    test_hardware_event_configuration();
    test_reset_stopped_counters(smp + 1);
    test_stop_reset_partial_batch();
    test_skip_match_validation(smp + 1);
    test_hardware_cycles();
    test_rejected_batch_preserves_cycle();
    test_access_fault_counters();
    test_firmware_ipi_counter(smp + 1);
}

fn report_counters() {
    let counters_num = sbi::pmu_num_counters();
    println!("[pmu] counters number: {}", counters_num);
    for idx in 0..counters_num {
        let info = sbi::pmu_counter_get_info(idx);
        // Skip indices reported as invalid by the SBI implementation.
        if info.error == RET_ERR_INVALID_PARAM {
            continue;
        }
        assert!(info.is_ok());
        let counter_info = CounterInfo::new(info.value);
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

/// Runs a real trap while its firmware event counter is active.
pub(crate) fn test_trap_counter(event: FirmwareEvent, trigger: impl FnOnce()) {
    if sbi::probe_extension(sbi::Pmu).is_unavailable() {
        trigger();
        return;
    }
    let counter = configure_event(
        EventIdx::new_firmware_event(event).raw(),
        ConfigFlags::CLEAR_VALUE | ConfigFlags::AUTO_START,
    );
    trigger();
    if let Some(counter) = counter {
        let value = sbi::pmu_counter_fw_read(counter.value);
        let stopped = sbi::pmu_counter_stop(
            CounterMask::from_mask_base(1, counter.value),
            StopFlags::RESET,
        );
        assert!(stopped.is_ok());
        assert!(
            value.is_ok() && value.value > 0,
            "PMU firmware trap event {event:?} stayed zero after its exception"
        );
    }
}

/// Find an available counter of the right type before exercising an event.
fn configure_event(event: usize, flags: ConfigFlags) -> Option<SbiRet> {
    let firmware = event >> 16 == 0xf;
    for idx in 0..sbi::pmu_num_counters() {
        let info = sbi::pmu_counter_get_info(idx);
        // Skip indices reported as invalid by the SBI implementation.
        if info.error == RET_ERR_INVALID_PARAM {
            continue;
        }
        assert!(info.is_ok());
        let info = CounterInfo::new(info.value);
        if info.is_firmware_counter() != firmware {
            continue;
        }
        // Fixed counters may already be running when the kernel boots.
        if !firmware && matches!(info.get_csr(), 0xc00 | 0xc02) {
            let result =
                sbi::pmu_counter_stop(CounterMask::from_mask_base(1, idx), StopFlags::empty());
            assert!(result.is_ok() || result.error == RET_ERR_ALREADY_STOPPED);
        }
        // A one-counter mask also works for indices beyond XLEN.
        let result =
            sbi::pmu_counter_config_matching(CounterMask::from_mask_base(1, idx), flags, event, 0);
        if result.error == RET_ERR_NOT_SUPPORTED {
            continue;
        }
        assert!(result.is_ok(), "PMU event {event:#x}: {result:?}");
        assert_eq!(result.value, idx);
        return Some(result);
    }
    println!("[pmu] no counter for event {event:#x}, skipping");
    None
}

fn test_hardware_event_configuration() {
    let flags = ConfigFlags::CLEAR_VALUE | ConfigFlags::AUTO_START;
    for event in [0x2, 0x10019, 0x1001b, 0x10021, 0x3] {
        let Some(result) = configure_event(event, flags) else {
            continue;
        };
        assert!(
            sbi::pmu_counter_stop(
                CounterMask::from_mask_base(1, result.value),
                StopFlags::RESET
            )
            .is_ok()
        );
    }
}

fn test_reset_stopped_counters(invalid_hart: usize) {
    for event in [
        0x10019,
        EventIdx::new_firmware_event(FirmwareEvent::IpiSent).raw(),
    ] {
        let Some(configured) = configure_event(event, ConfigFlags::empty()) else {
            continue;
        };
        let index = configured.value;
        let selected = CounterMask::from_mask_base(1, index);
        assert_eq!(
            sbi::pmu_counter_stop(selected, StopFlags::RESET),
            SbiRet::already_stopped()
        );

        let info = sbi::pmu_counter_get_info(index);
        assert!(info.is_ok());
        let info = CounterInfo::new(info.value);
        if info.is_firmware_counter() {
            // Starting without reconfiguring proves RESET kept the binding.
            assert_eq!(
                sbi::pmu_counter_start(selected, StartFlags::INIT_VALUE, 7),
                SbiRet::success(0)
            );
            assert_eq!(
                sbi::send_ipi(HartMask::from_mask_base(1, invalid_hart)),
                SbiRet::invalid_param()
            );
            assert_eq!(sbi::pmu_counter_fw_read(index), SbiRet::success(8));
        } else {
            // A large initial value exposes an erroneous value clear below.
            let initial: u64 = 1_000_000;
            assert_eq!(
                sbi::pmu_counter_start(selected, StartFlags::INIT_VALUE, initial),
                SbiRet::success(0)
            );
            assert!(read_hardware_counter(info.get_csr()) >= initial);
        }
        assert!(sbi::pmu_counter_stop(selected, StopFlags::RESET).is_ok());
    }
    println!("[pmu] STOP+RESET retains bindings of already-stopped counters");
}

fn test_stop_reset_partial_batch() {
    let count = sbi::pmu_num_counters();
    if count < 2 {
        return;
    }
    let stopped = count - 2;
    let running = count - 1;
    let stopped_info = sbi::pmu_counter_get_info(stopped);
    let running_info = sbi::pmu_counter_get_info(running);
    if stopped_info.is_err()
        || running_info.is_err()
        || !CounterInfo::new(stopped_info.value).is_firmware_counter()
        || !CounterInfo::new(running_info.value).is_firmware_counter()
    {
        return;
    }

    let fw_event = EventIdx::new_firmware_event(FirmwareEvent::IpiSent).raw();
    assert_eq!(
        sbi::pmu_counter_config_matching(
            CounterMask::from_mask_base(1, stopped),
            ConfigFlags::empty(),
            fw_event,
            0,
        ),
        SbiRet::success(stopped)
    );
    assert_eq!(
        sbi::pmu_counter_config_matching(
            CounterMask::from_mask_base(1, running),
            ConfigFlags::empty(),
            EventIdx::new_firmware_event(FirmwareEvent::AccessLoad).raw(),
            0,
        ),
        SbiRet::success(running)
    );
    assert_eq!(
        sbi::pmu_counter_start(
            CounterMask::from_mask_base(1, running),
            StartFlags::INIT_VALUE,
            5,
        ),
        SbiRet::success(0)
    );

    // The stopped member keeps its binding; the running member still resets.
    assert_eq!(
        sbi::pmu_counter_stop(CounterMask::from_mask_base(0b11, stopped), StopFlags::RESET,),
        SbiRet::already_stopped()
    );
    assert_eq!(sbi::pmu_counter_fw_read(stopped), SbiRet::success(0));
    assert_eq!(
        sbi::pmu_counter_start(
            CounterMask::from_mask_base(1, stopped),
            StartFlags::empty(),
            0,
        ),
        SbiRet::success(0),
        "a stopped counter in the batch lost its binding"
    );
    assert_eq!(sbi::pmu_counter_fw_read(running), SbiRet::invalid_param());
    assert_eq!(
        sbi::pmu_counter_start(
            CounterMask::from_mask_base(1, running),
            StartFlags::empty(),
            0,
        ),
        SbiRet::invalid_param(),
        "a running counter in the batch kept its binding after RESET"
    );
    assert!(
        sbi::pmu_counter_stop(CounterMask::from_mask_base(1, stopped), StopFlags::RESET).is_ok()
    );
    println!("[pmu] STOP+RESET preserves the stopped member of a batch");
}

fn test_skip_match_validation(invalid_hart: usize) {
    // A firmware event sent to a selected hardware counter must not clear or
    // bind that running counter.
    if let Some(configured) = configure_event(0x10019, ConfigFlags::CLEAR_VALUE) {
        let index = configured.value;
        let info = sbi::pmu_counter_get_info(index);
        assert!(info.is_ok());
        let info = CounterInfo::new(info.value);
        assert!(!info.is_firmware_counter());
        let selected = CounterMask::from_mask_base(1, index);
        let initial: u64 = 1_000_000;
        assert_eq!(
            sbi::pmu_counter_start(selected, StartFlags::INIT_VALUE, initial),
            SbiRet::success(0)
        );
        assert!(read_hardware_counter(info.get_csr()) >= initial);

        assert_eq!(
            sbi::pmu_counter_config_matching(
                selected,
                ConfigFlags::SKIP_MATCH | ConfigFlags::CLEAR_VALUE,
                EventIdx::new_firmware_event(FirmwareEvent::IpiSent).raw(),
                0,
            ),
            SbiRet::not_supported(),
            "SKIP_MATCH accepted a firmware event on a hardware counter"
        );
        assert!(
            read_hardware_counter(info.get_csr()) >= initial,
            "a rejected SKIP_MATCH cleared the hardware counter"
        );
        assert!(sbi::pmu_counter_stop(selected, StopFlags::RESET).is_ok());
    }

    // A hardware event sent to a selected firmware counter must not clear,
    // reconfigure, or restart that running counter.
    if let Some(configured) = configure_event(
        EventIdx::new_firmware_event(FirmwareEvent::IpiSent).raw(),
        ConfigFlags::CLEAR_VALUE,
    ) {
        let index = configured.value;
        let selected = CounterMask::from_mask_base(1, index);
        assert_eq!(
            sbi::pmu_counter_start(selected, StartFlags::INIT_VALUE, 41),
            SbiRet::success(0)
        );
        assert_eq!(
            sbi::send_ipi(HartMask::from_mask_base(1, invalid_hart)),
            SbiRet::invalid_param()
        );
        assert_eq!(sbi::pmu_counter_fw_read(index), SbiRet::success(42));

        assert_eq!(
            sbi::pmu_counter_config_matching(
                selected,
                ConfigFlags::SKIP_MATCH | ConfigFlags::CLEAR_VALUE | ConfigFlags::AUTO_START,
                0x1,
                0,
            ),
            SbiRet::not_supported(),
            "SKIP_MATCH accepted a hardware event on a firmware counter"
        );
        assert_eq!(
            sbi::pmu_counter_fw_read(index),
            SbiRet::success(42),
            "a rejected SKIP_MATCH cleared the firmware counter"
        );
        assert!(
            sbi::pmu_counter_stop(selected, StopFlags::RESET).is_ok(),
            "a rejected SKIP_MATCH stopped the firmware counter"
        );
    }
    println!("[pmu] SKIP_MATCH rejects firmware/hardware class mismatches");
}

fn test_hardware_cycles() {
    // `SBI_PMU_HW_CPU_CYCLES` event.
    let Some(result) = configure_event(0x1, ConfigFlags::CLEAR_VALUE) else {
        return;
    };
    assert!(result.is_ok());
    let cycle_counter_idx = result.value;

    let counter_info = sbi::pmu_counter_get_info(cycle_counter_idx);
    assert!(counter_info.is_ok());
    let counter_info = CounterInfo::new(counter_info.value);
    assert!(!counter_info.is_firmware_counter());
    let cycle_counter_csr = counter_info.get_csr();

    // A nonzero upper word also checks RV32's split machine-counter write.
    let initial = 0x1_0000_ffff;
    let start_result = sbi::pmu_counter_start(
        CounterMask::from_mask_base(0x1, cycle_counter_idx),
        StartFlags::INIT_VALUE,
        initial,
    );
    assert!(start_result.is_ok());
    let cycle_num = read_hardware_counter(cycle_counter_csr);
    assert!(cycle_num >= initial);
    assert_eq!(
        sbi::pmu_counter_start(
            CounterMask::from_mask_base(1, cycle_counter_idx),
            StartFlags::INIT_VALUE,
            0,
        ),
        SbiRet::already_started()
    );
    assert!(read_hardware_counter(cycle_counter_csr) >= initial);

    let stop_result = sbi::pmu_counter_stop(
        CounterMask::from_mask_base(0x1, cycle_counter_idx),
        StopFlags::empty(),
    );
    assert!(stop_result.is_ok());
    assert_eq!(
        sbi::pmu_counter_stop(
            CounterMask::from_mask_base(1, cycle_counter_idx),
            StopFlags::empty(),
        ),
        SbiRet::already_stopped()
    );
    let mut _sum = 0;
    for i in 0..1000 {
        _sum += i;
    }
    let stopped_cycle_num = read_hardware_counter(cycle_counter_csr);

    // Restarting with update=0 must resume from the stopped value.
    let start_result = sbi::pmu_counter_start(
        CounterMask::from_mask_base(0x1, cycle_counter_idx),
        StartFlags::empty(),
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

fn test_rejected_batch_preserves_cycle() {
    let Some(cycle) = (0..sbi::pmu_num_counters()).find(|&index| {
        let info = sbi::pmu_counter_get_info(index);
        info.is_ok()
            && !CounterInfo::new(info.value).is_firmware_counter()
            && CounterInfo::new(info.value).get_csr() == 0xc00
    }) else {
        return;
    };
    let selected = CounterMask::from_mask_base(1, cycle);
    let stopped = sbi::pmu_counter_stop(selected, StopFlags::empty());
    assert!(stopped.is_ok() || stopped == SbiRet::already_stopped());

    // A dense logical table has no internal gaps. Select its last firmware
    // counter together with the first invalid index; this also fits RV32.
    let last = sbi::pmu_num_counters() - 1;
    assert!(CounterInfo::new(sbi::pmu_counter_get_info(last).value).is_firmware_counter());
    let firmware = CounterMask::from_mask_base(1, last);
    assert_eq!(
        sbi::pmu_counter_start(firmware, StartFlags::empty(), 0),
        SbiRet::invalid_param(),
        "unbound firmware counter started before configuration"
    );
    let configured = sbi::pmu_counter_config_matching(
        firmware,
        ConfigFlags::empty(),
        EventIdx::new_firmware_event(FirmwareEvent::IpiSent).raw(),
        0,
    );
    assert_eq!(configured, SbiRet::success(last));
    assert_eq!(
        sbi::pmu_counter_start(CounterMask::from_mask_base(3, last), StartFlags::empty(), 0),
        SbiRet::invalid_param()
    );
    assert_eq!(
        sbi::pmu_counter_start(firmware, StartFlags::empty(), 0),
        SbiRet::success(0),
        "invalid PMU batch started its earlier valid counter"
    );
    assert!(sbi::pmu_counter_stop(firmware, StopFlags::RESET).is_ok());
    assert_eq!(
        sbi::pmu_counter_start(
            selected,
            StartFlags::INIT_VALUE | StartFlags::INIT_SNAPSHOT,
            0
        ),
        SbiRet::invalid_param()
    );
    assert!(sbi::pmu_counter_start(selected, StartFlags::empty(), 0).is_ok());
    println!("[pmu] rejected batches preserve counter state");
}

fn test_access_fault_counters() {
    for event in [FirmwareEvent::AccessLoad, FirmwareEvent::AccessStore] {
        let Some(result) = configure_event(
            EventIdx::new_firmware_event(event).raw(),
            ConfigFlags::CLEAR_VALUE | ConfigFlags::AUTO_START,
        ) else {
            return;
        };
        assert!(result.is_ok());
        let info = sbi::pmu_counter_get_info(result.value);
        assert!(info.is_ok() && CounterInfo::new(info.value).is_firmware_counter());
        assert_eq!(sbi::pmu_counter_fw_read(result.value), SbiRet::success(0));
        for count in 1..=2 {
            crate::trap::access_fault(event == FirmwareEvent::AccessStore);
            assert_eq!(
                sbi::pmu_counter_fw_read(result.value),
                SbiRet::success(count)
            );
        }
        assert!(
            sbi::pmu_counter_stop(
                CounterMask::from_mask_base(1, result.value),
                StopFlags::empty()
            )
            .is_ok()
        );
        crate::trap::access_fault(event == FirmwareEvent::AccessStore);
        assert_eq!(sbi::pmu_counter_fw_read(result.value), SbiRet::success(2));
        assert!(
            sbi::pmu_counter_start(
                CounterMask::from_mask_base(1, result.value),
                StartFlags::empty(),
                0
            )
            .is_ok()
        );
        assert!(
            sbi::pmu_counter_stop(
                CounterMask::from_mask_base(1, result.value),
                StopFlags::RESET
            )
            .is_ok()
        );
    }
    println!("[pmu] access-fault counters pass");
}

fn test_firmware_ipi_counter(invalid_hart: usize) {
    // IPI_SENT is a firmware counter and starts at zero.
    let Some(result) = configure_event(
        EventIdx::new_firmware_event(FirmwareEvent::IpiSent).raw(),
        ConfigFlags::CLEAR_VALUE,
    ) else {
        return;
    };
    assert!(result.is_ok());
    let info = sbi::pmu_counter_get_info(result.value);
    assert!(info.is_ok() && CounterInfo::new(info.value).is_firmware_counter());
    let ipi_counter_idx = result.value;
    let ipi_num = sbi::pmu_counter_fw_read(ipi_counter_idx);
    assert!(ipi_num.is_ok());
    assert_eq!(ipi_num.value, 0);

    // Updating the counter while starting it sets the requested initial value.
    let start_result = sbi::pmu_counter_start(
        CounterMask::from_mask_base(0x1, ipi_counter_idx),
        StartFlags::INIT_VALUE,
        25,
    );
    assert!(start_result.is_ok());
    let ipi_num = sbi::pmu_counter_fw_read(ipi_counter_idx);
    assert!(ipi_num.is_ok());
    assert_eq!(ipi_num.value, 25);
    assert_eq!(
        sbi::pmu_counter_start(
            CounterMask::from_mask_base(1, ipi_counter_idx),
            StartFlags::INIT_VALUE,
            99,
        ),
        SbiRet::already_started()
    );
    assert_eq!(
        sbi::pmu_counter_fw_read(ipi_counter_idx),
        SbiRet::success(25)
    );

    // IPI_SENT counts send attempts, including rejected targets.
    let send_ipi_result = sbi::send_ipi(HartMask::from_mask_base(0x1, invalid_hart));
    assert_eq!(send_ipi_result, SbiRet::invalid_param());
    let ipi_num = sbi::pmu_counter_fw_read(ipi_counter_idx);
    assert!(ipi_num.is_ok());
    assert_eq!(ipi_num.value, 26);

    let stop_result = sbi::pmu_counter_stop(
        CounterMask::from_mask_base(0x1, ipi_counter_idx),
        StopFlags::empty(),
    );
    assert!(stop_result.is_ok());
    assert_eq!(
        sbi::pmu_counter_stop(
            CounterMask::from_mask_base(0x1, ipi_counter_idx),
            StopFlags::empty(),
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
        StartFlags::empty(),
        0,
    );
    assert!(start_result.is_ok());
    let send_ipi_result = sbi::send_ipi(HartMask::from_mask_base(0x1, invalid_hart));
    assert_eq!(send_ipi_result, SbiRet::invalid_param());
    let ipi_num = sbi::pmu_counter_fw_read(ipi_counter_idx);
    assert!(ipi_num.is_ok());
    assert_eq!(ipi_num.value, 27);
}

/// Leaves a bound, running firmware counter across a non-retentive suspend.
pub(crate) fn prepare_resume_counter() -> Option<usize> {
    if sbi::probe_extension(sbi::Pmu).is_unavailable() {
        return None;
    }
    let counter = configure_event(
        EventIdx::new_firmware_event(FirmwareEvent::IpiSent).raw(),
        ConfigFlags::empty(),
    )
    .expect("selected PMU exposes firmware IPI counters")
    .value;
    assert_eq!(
        sbi::pmu_counter_start(
            CounterMask::from_mask_base(1, counter),
            StartFlags::INIT_VALUE,
            25
        ),
        SbiRet::success(0)
    );
    assert_eq!(sbi::pmu_counter_fw_read(counter), SbiRet::success(25));
    Some(counter)
}

/// Checks Runtime reset both the binding and value before entering the resume stage.
pub(crate) fn check_resume_counter(counter: usize) {
    assert_eq!(sbi::pmu_counter_fw_read(counter), SbiRet::invalid_param());
    let selected = CounterMask::from_mask_base(1, counter);
    assert_eq!(
        sbi::pmu_counter_config_matching(
            selected,
            ConfigFlags::empty(),
            EventIdx::new_firmware_event(FirmwareEvent::IpiSent).raw(),
            0
        ),
        SbiRet::success(counter)
    );
    assert_eq!(sbi::pmu_counter_fw_read(counter), SbiRet::success(0));
    assert_eq!(
        sbi::pmu_counter_start(selected, StartFlags::empty(), 0),
        SbiRet::success(0)
    );
    assert_eq!(
        sbi::pmu_counter_stop(selected, StopFlags::RESET),
        SbiRet::success(0)
    );
    println!("[pmu] non-retentive resume resets bindings and counts");
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

/// Decodes the packed value returned by `pmu_counter_get_info`.
struct CounterInfo {
    /// Bits [11:0] hold the CSR number, [17:12] the width, and the MSB marks a
    /// firmware counter.
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
    const fn new_firmware_event(event: FirmwareEvent) -> Self {
        Self {
            inner: 0xf << 16 | event.code(),
        }
    }

    const fn raw(&self) -> usize {
        self.inner
    }
}
