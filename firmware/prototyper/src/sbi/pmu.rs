//! Performance monitoring.
//!
//! # References
//!
//! - Specification: [RISC-V SBI PMU extension](https://docs.riscv.org/reference/sbi/v3.0/ext-pmu.html) —
//!   counter discovery, event configuration, and shared-memory operations.

use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use csr::{
    machine,
    user::Cycle,
    values::counter::{HardwareCounter, InhibitedCounters},
};
use runtime::{
    FdtNode, Result,
    events::{Exception, TrapCounters},
    hart::{HartId, PerHart},
    rustsbi::{Pmu, SbiRet},
};
use sbi_spec::binary::SharedPtr;
use sbi_spec::pmu::shmem_size::SIZE;
use sbi_spec::pmu::*;
use spin::{Mutex, Once};

use super::features::{PrivilegedVersion, counter_topology, mhpm_mask, privileged_version};

/// Maximum number of hardware performance counters supported.
const PMU_HARDWARE_COUNTER_MAX: usize = 32;
/// Maximum number of firmware-managed counters supported.
const PMU_FIRMWARE_COUNTER_MAX: usize = 16;
/// Marker value for inactive/invalid event indices.
const PMU_EVENT_IDX_INVALID: usize = usize::MAX;

/// Per-hart PMU records, published before platform discovery.
static PMU_STATES: Once<PerHart<Mutex<PmuState>>> = Once::new();

/// Creates one PMU record per enabled hart on the boot hart.
///
/// The caller must run this after Runtime publishes the hart topology and
/// before any SBI service observes PMU state.
pub(crate) fn init() {
    PMU_STATES.call_once(|| {
        PerHart::new(|_| Mutex::new(PmuState::new(0)))
            .expect("failed to initialize per-hart PMU state")
    });
}

fn state(hart: HartId) -> &'static Mutex<PmuState> {
    PMU_STATES
        .get()
        .expect("BUG: PMU state used before initialization")
        .get(hart)
}

fn current_state() -> &'static Mutex<PmuState> {
    state(HartId::current().expect("BUG: current hart is not in the boot topology"))
}

/// Rebuilds the current hart's PMU state using the detected counter mask.
pub(crate) fn init_current(mhpm_mask: u32) {
    let hart = HartId::current().expect("BUG: current hart is not in the boot topology");
    *state(hart).lock() = PmuState::new(mhpm_mask);
}

/// Resets the current hart's PMU state before a non-retentive resume.
///
/// Pending IPIs and fence requests remain valid across suspend/resume.
pub(crate) fn reset_current() {
    let hart = HartId::current().expect("BUG: current hart is not in the boot topology");
    let (privileged_version, mhpm_mask) = counter_topology(hart);
    if privileged_version >= PrivilegedVersion::Version1_11 {
        // SAFETY: This hart owns its PMU state while resuming. The PMU does not
        // coordinate cycle-counter control shared with other harts.
        unsafe { machine::CounterInhibit::write(InhibitedCounters::programmable()) };
    }
    *state(hart).lock() = PmuState::new(mhpm_mask);
}

/// PMU state for hardware and firmware performance counters.
#[repr(C)]
pub struct PmuState {
    active_event: [usize; PMU_HARDWARE_COUNTER_MAX + PMU_FIRMWARE_COUNTER_MAX],
    /// Bitmap of active firmware counters, with one bit per counter.
    fw_counter_state: usize,
    /// Values of firmware-managed counters.
    fw_counter: [u64; PMU_FIRMWARE_COUNTER_MAX],
    hw_counters_num: usize,
    total_counters_num: usize,
}

impl PmuState {
    /// Creates PMU state for the given hardware counter mask.
    pub fn new(mhpm_mask: u32) -> Self {
        let hw_counters_num = mhpm_mask.count_ones() as usize;
        let total_counters_num = hw_counters_num + PMU_FIRMWARE_COUNTER_MAX;

        let mut active_event =
            [PMU_EVENT_IDX_INVALID; PMU_HARDWARE_COUNTER_MAX + PMU_FIRMWARE_COUNTER_MAX];
        // Fixed counters have architectural event assignments.
        active_event[0] = hardware_event::CPU_CYCLES;
        active_event[2] = hardware_event::INSTRUCTIONS;

        Self {
            active_event,
            fw_counter_state: 0,
            fw_counter: [0; PMU_FIRMWARE_COUNTER_MAX],
            hw_counters_num,
            total_counters_num,
        }
    }

    /// Returns the number of hardware counters available.
    #[inline(always)]
    pub fn get_hw_counter_num(&self) -> usize {
        self.hw_counters_num
    }

    /// Returns the total number of counters (hardware + firmware).
    #[inline(always)]
    pub fn get_total_counters_num(&self) -> usize {
        self.total_counters_num
    }

    /// Returns the event index associated with a counter.
    #[inline]
    pub fn get_event_idx(&self, counter_idx: usize, firmware_event: bool) -> Option<EventIdx> {
        if counter_idx >= self.total_counters_num {
            return None;
        }
        if firmware_event && counter_idx < self.hw_counters_num {
            return None;
        }

        Some(EventIdx::new(self.active_event[counter_idx]))
    }

    /// Returns the value of a firmware counter.
    #[inline]
    pub fn get_fw_counter(&self, counter_idx: usize) -> Option<u64> {
        if counter_idx < self.hw_counters_num || counter_idx >= self.total_counters_num {
            return None;
        }
        let fw_idx = counter_idx - self.hw_counters_num;
        Some(self.fw_counter[fw_idx])
    }

    /// Starts a firmware counter, optionally setting its initial value.
    #[inline]
    fn start_fw_counter(
        &mut self,
        counter_idx: usize,
        initial_value: u64,
        is_update_value: bool,
    ) -> Result<(), StartCounterErr> {
        if counter_idx < self.hw_counters_num || counter_idx >= self.total_counters_num {
            return Err(StartCounterErr::OffsetInvalid);
        }
        let fw_idx = counter_idx - self.hw_counters_num;

        if self.fw_counter_state & (1 << fw_idx) != 0 {
            return Err(StartCounterErr::AlreadyStart);
        }

        if is_update_value {
            self.fw_counter[fw_idx] = initial_value;
        }
        self.fw_counter_state |= 1 << fw_idx;
        Ok(())
    }

    /// Stops a firmware counter and optionally clears its event binding.
    #[inline]
    fn stop_fw_counter(
        &mut self,
        counter_idx: usize,
        is_reset: bool,
    ) -> Result<(), StopCounterErr> {
        if counter_idx < self.hw_counters_num || counter_idx >= self.total_counters_num {
            return Err(StopCounterErr::OffsetInvalid);
        }
        let fw_idx = counter_idx - self.hw_counters_num;

        if self.fw_counter_state & (1 << fw_idx) == 0 {
            return Err(StopCounterErr::AlreadyStop);
        }

        if is_reset {
            self.active_event[counter_idx] = PMU_EVENT_IDX_INVALID;
        }
        self.fw_counter_state &= !(1 << fw_idx);
        Ok(())
    }

    #[inline]
    pub fn is_firmware_event_start(&self, counter_idx: usize) -> bool {
        if counter_idx < self.hw_counters_num || counter_idx >= self.total_counters_num {
            return false;
        }
        let fw_idx = counter_idx - self.hw_counters_num;
        self.fw_counter_state & (1 << fw_idx) != 0
    }
}

#[derive(Default)]
pub struct SbiPmu {
    event_to_mhpmevent: Option<BTreeMap<u32, u64>>,
    event_to_mhpmcounter: Option<Vec<EventToCounterMap>>,
    raw_event_to_mhpmcounter: Option<Vec<RawEventToCounterMap>>,
}

/// An adapter that records Runtime trap events in firmware PMU counters.
struct RuntimeTrapCounters;

static RUNTIME_TRAP_COUNTERS: RuntimeTrapCounters = RuntimeTrapCounters;

impl TrapCounters for RuntimeTrapCounters {
    fn record(&self, exception: Exception) {
        let event = match exception {
            Exception::IllegalInstruction => firmware_event::ILLEGAL_INSN,
            Exception::LoadMisaligned => firmware_event::MISALIGNED_LOAD,
            Exception::StoreMisaligned => firmware_event::MISALIGNED_STORE,
            Exception::LoadAccess => firmware_event::ACCESS_LOAD,
            Exception::StoreAccess => firmware_event::ACCESS_STORE,
            _ => return,
        };
        pmu_firmware_counter_increment(event);
    }
}

/// Returns the firmware PMU counter sink for Runtime trap events.
pub(crate) fn runtime_counters() -> &'static dyn TrapCounters {
    &RUNTIME_TRAP_COUNTERS
}

impl Pmu for SbiPmu {
    /// Returns the total number of available performance counters.
    ///
    /// Implements SBI PMU function 0.
    #[inline]
    fn num_counters(&self) -> usize {
        current_state().lock().get_total_counters_num()
    }

    /// Returns a counter's CSR, encoded width, and type (SBI PMU function 1).
    #[inline]
    fn counter_get_info(&self, counter_idx: usize) -> SbiRet {
        let hart = HartId::current().expect("BUG: current hart exceeds Runtime capacity");
        let pmu_state = state(hart).lock();
        if counter_idx >= pmu_state.get_total_counters_num() {
            return SbiRet::invalid_param();
        }
        let hw_counters_num = pmu_state.get_hw_counter_num();
        drop(pmu_state);

        if counter_idx < hw_counters_num {
            let mask = mhpm_mask(hart);

            // Set bits identify implemented counters in logical-index order.
            let mut remaining_mask = mask;
            let mut count = 0;

            while remaining_mask != 0 {
                if count == counter_idx {
                    let offset = remaining_mask.trailing_zeros() as u16;
                    // `time` cannot be started or stopped through the PMU extension.
                    if offset == 1 {
                        return SbiRet::invalid_param();
                    }
                    return SbiRet::success(
                        CounterInfo::with_hardware_info(Cycle::NUMBER + offset, 63).inner(),
                    );
                }
                remaining_mask &= remaining_mask - 1;
                count += 1;
            }
            return SbiRet::invalid_param();
        }

        SbiRet::success(CounterInfo::with_firmware_info().inner())
    }

    /// Finds and configures a matching counter (SBI PMU function 2).
    #[inline]
    fn counter_config_matching(
        &self,
        counter_idx_base: usize,
        counter_idx_mask: usize,
        config_flags: usize,
        event_idx: usize,
        event_data: u64,
    ) -> SbiRet {
        let flags = match flags::ConfigFlags::from_bits(config_flags) {
            Some(flags) => flags,
            None => return SbiRet::invalid_param(), // Reserved bits are set.
        };

        let event = EventIdx::new(event_idx);
        let is_firmware_event = event.is_firmware_event();

        let mut pmu_state = current_state().lock();
        if counter_idx_base >= pmu_state.total_counters_num
            || counter_idx_mask == 0
            || counter_idx_mask
                .checked_shr((pmu_state.total_counters_num - counter_idx_base) as u32)
                .unwrap_or(0)
                != 0
            || !event.check_event_type()
            || (is_firmware_event && !event.firmware_event_valid())
        {
            return SbiRet::invalid_param();
        }

        let skip_match = flags.contains(flags::ConfigFlags::SKIP_MATCH);

        let counter_idx;

        if skip_match {
            if let Some(ctr_idx) = CounterMask::new(counter_idx_base, counter_idx_mask).next() {
                if pmu_state.active_event[ctr_idx] == PMU_EVENT_IDX_INVALID {
                    return SbiRet::invalid_param();
                }
                counter_idx = ctr_idx;
            } else {
                return SbiRet::invalid_param();
            }
        } else {
            let match_result = if event.is_firmware_event() {
                self.find_firmware_counter(
                    counter_idx_base,
                    counter_idx_mask,
                    event_idx,
                    &pmu_state,
                )
            } else {
                self.find_hardware_counter(
                    counter_idx_base,
                    counter_idx_mask,
                    event_idx,
                    event_data,
                    &pmu_state,
                )
            };
            match match_result {
                Ok(ctr_idx) => {
                    counter_idx = ctr_idx;
                }
                Err(err) => {
                    return err;
                }
            }
            pmu_state.active_event[counter_idx] = event_idx;
        }

        match configure_counter(&mut pmu_state, counter_idx, event, flags) {
            Ok(_) => SbiRet::success(counter_idx),
            Err(e) => e,
        }
    }

    /// Starts one or more counters (SBI PMU function 3).
    #[inline]
    fn counter_start(
        &self,
        counter_idx_base: usize,
        counter_idx_mask: usize,
        start_flags: usize,
        initial_value: u64,
    ) -> SbiRet {
        let flags = match flags::StartFlags::from_bits(start_flags) {
            Some(flags) => flags,
            None => return SbiRet::invalid_param(),
        };

        let mut pmu_state = current_state().lock();
        let is_update_value = flags.contains(flags::StartFlags::INIT_VALUE);

        if counter_idx_base >= pmu_state.total_counters_num
            || counter_idx_mask == 0
            || counter_idx_mask
                .checked_shr((pmu_state.total_counters_num - counter_idx_base) as u32)
                .unwrap_or(0)
                != 0
        {
            return SbiRet::invalid_param();
        }

        if flags.contains(flags::StartFlags::INIT_SNAPSHOT) {
            return SbiRet::no_shmem();
        }

        for counter_idx in CounterMask::new(counter_idx_base, counter_idx_mask) {
            if counter_idx >= pmu_state.total_counters_num {
                return SbiRet::invalid_param();
            }

            let start_result = if counter_idx >= pmu_state.get_hw_counter_num() {
                pmu_state.start_fw_counter(counter_idx, initial_value, is_update_value)
            } else {
                let mhpm_offset = get_mhpm_csr_offset(counter_idx).unwrap();
                start_hardware_counter(mhpm_offset, initial_value, is_update_value)
            };
            match start_result {
                Ok(_) => {}
                Err(StartCounterErr::AlreadyStart) => {
                    return SbiRet::already_started();
                }
                Err(StartCounterErr::OffsetInvalid) => {
                    return SbiRet::invalid_param();
                }
            }
        }
        SbiRet::success(0)
    }

    /// Stops one or more counters (SBI PMU function 4).
    #[inline]
    fn counter_stop(
        &self,
        counter_idx_base: usize,
        counter_idx_mask: usize,
        stop_flags: usize,
    ) -> SbiRet {
        let flags = match flags::StopFlags::from_bits(stop_flags) {
            Some(flags) => flags,
            None => return SbiRet::invalid_param(),
        };

        let mut pmu_state = current_state().lock();
        let is_reset = flags.contains(flags::StopFlags::RESET);

        if counter_idx_base >= pmu_state.total_counters_num
            || counter_idx_mask == 0
            || counter_idx_mask
                .checked_shr((pmu_state.total_counters_num - counter_idx_base) as u32)
                .unwrap_or(0)
                != 0
        {
            return SbiRet::invalid_param();
        }

        if flags.contains(flags::StopFlags::TAKE_SNAPSHOT) {
            return SbiRet::no_shmem();
        }

        let mut result = SbiRet::invalid_param();
        for counter_idx in CounterMask::new(counter_idx_base, counter_idx_mask) {
            if counter_idx >= pmu_state.total_counters_num {
                return SbiRet::invalid_param();
            }

            let stop_result = if counter_idx >= pmu_state.get_hw_counter_num() {
                pmu_state.stop_fw_counter(counter_idx, is_reset)
            } else {
                let mhpm_offset = get_mhpm_csr_offset(counter_idx).unwrap();
                // Keep stopping the other selected counters across the time hole.
                if mhpm_offset == 1 {
                    continue;
                }
                if is_reset {
                    pmu_state.active_event[counter_idx] = PMU_EVENT_IDX_INVALID;
                }
                stop_hardware_counter(mhpm_offset, is_reset)
            };
            match stop_result {
                Ok(_) => result = SbiRet::success(0),
                Err(StopCounterErr::OffsetInvalid) => return SbiRet::invalid_param(),
                Err(StopCounterErr::AlreadyStop) => return SbiRet::already_stopped(),
            }
        }
        result
    }

    /// Reads a firmware counter (SBI PMU function 5).
    #[inline]
    fn counter_fw_read(&self, counter_idx: usize) -> SbiRet {
        let pmu_state = current_state().lock();
        match pmu_state.get_event_idx(counter_idx, true) {
            Some(event_id) if event_id.firmware_event_valid() => {
                if event_id.event_code() == firmware_event::PLATFORM {
                    // TODO: Handle platform-specific PMU events.
                    return SbiRet::invalid_param();
                }
                match pmu_state.get_fw_counter(counter_idx) {
                    Some(value) => SbiRet::success(value as usize),
                    None => SbiRet::invalid_param(),
                }
            }
            _ => SbiRet::invalid_param(),
        }
    }

    /// Reads a firmware counter's high bits (SBI PMU function 6).
    #[inline]
    fn counter_fw_read_hi(&self, counter_idx: usize) -> SbiRet {
        #[cfg(target_pointer_width = "64")]
        {
            let _ = counter_idx;
            SbiRet::success(0)
        }
        #[cfg(target_pointer_width = "32")]
        {
            let ret = self.counter_fw_read(counter_idx);
            if ret.is_err() {
                return ret;
            }
            let pmu_state = current_state().lock();
            match pmu_state.get_fw_counter(counter_idx) {
                Some(value) => SbiRet::success((value >> 32) as usize),
                None => SbiRet::invalid_param(),
            }
        }
    }

    /// Reports that PMU snapshot shared memory is unsupported (SBI PMU function 7).
    #[inline]
    fn snapshot_set_shmem(&self, shmem: SharedPtr<[u8; SIZE]>, flags: usize) -> SbiRet {
        let _ = (shmem, flags);
        SbiRet::not_supported()
    }
}

impl SbiPmu {
    fn find_firmware_counter(
        &self,
        counter_idx_base: usize,
        counter_idx_mask: usize,
        event_idx: usize,
        pmu_state: &PmuState,
    ) -> Result<usize, SbiRet> {
        // TODO: Support the `PLATFORM` event.
        let event = EventIdx::new(event_idx);
        if !event.firmware_event_valid() {
            return Err(SbiRet::not_supported());
        }

        for counter_idx in CounterMask::new(counter_idx_base, counter_idx_mask) {
            if counter_idx < pmu_state.get_hw_counter_num()
                || counter_idx >= pmu_state.get_total_counters_num()
            {
                continue;
            }
            if pmu_state.active_event[counter_idx] != PMU_EVENT_IDX_INVALID {
                continue;
            }
            return Ok(counter_idx);
        }
        Err(SbiRet::not_supported())
    }

    fn find_hardware_counter(
        &self,
        counter_idx_base: usize,
        counter_idx_mask: usize,
        event_idx: usize,
        event_data: u64,
        pmu_state: &PmuState,
    ) -> Result<usize, SbiRet> {
        let event = EventIdx::new(event_idx);
        // Fixed counters have architectural event assignments even when
        // the device tree provides no programmable-counter mapping.
        let fixed_counter = CounterMask::new(counter_idx_base, counter_idx_mask).find(|&idx| {
            matches!(
                (event_idx, get_mhpm_csr_offset(idx)),
                (hardware_event::CPU_CYCLES, Some(0)) | (hardware_event::INSTRUCTIONS, Some(2))
            )
        });
        let mut hw_counters_mask = 0;
        if event.is_raw_event() {
            if let Some(ref raw_event_map_vec) = self.raw_event_to_mhpmcounter {
                for raw_event_map in raw_event_map_vec {
                    if raw_event_map.have_event(event_data) {
                        hw_counters_mask = raw_event_map.get_counter_mask();
                        break;
                    }
                }
            } else {
                return Err(SbiRet::not_supported());
            }
        } else {
            if let Some(ref sbi_hw_event_map_vec) = self.event_to_mhpmcounter {
                for sbi_hw_event_map in sbi_hw_event_map_vec {
                    if sbi_hw_event_map.have_event(event_idx as u32) {
                        hw_counters_mask = sbi_hw_event_map.get_counter_mask();
                        break;
                    }
                }
            } else {
                return fixed_counter.ok_or_else(SbiRet::not_supported);
            }
        }
        // The fixed counters cannot be used for arbitrary events.
        let hart = HartId::current().expect("BUG: current hart exceeds Runtime capacity");
        let (privileged_version, mhpm_mask) = counter_topology(hart);
        let can_use_counter_mask = hw_counters_mask & mhpm_mask;

        for counter_idx in CounterMask::new(counter_idx_base, counter_idx_mask) {
            if counter_idx >= pmu_state.hw_counters_num {
                continue;
            }

            let mhpm_offset = get_mhpm_csr_offset(counter_idx).unwrap();
            if (can_use_counter_mask >> mhpm_offset) & 0x1 == 0
                || pmu_state.active_event[counter_idx] != PMU_EVENT_IDX_INVALID
            {
                continue;
            }
            if (mhpm_offset == 0 && event_idx != 1) || (mhpm_offset == 2 && event_idx != 2) {
                continue;
            }
            if privileged_version >= PrivilegedVersion::Version1_11 {
                let Some(counter) = HardwareCounter::from_index(mhpm_offset) else {
                    continue;
                };
                if !machine::CounterInhibit::read().contains(counter) {
                    continue;
                }
            }

            self.pmu_update_hardware_mhpmevent(mhpm_offset, event_idx, event_data)?;
            return Ok(counter_idx);
        }
        fixed_counter.ok_or_else(SbiRet::not_supported)
    }

    fn pmu_update_hardware_mhpmevent(
        &self,
        mhpm_offset: u16,
        event_idx: usize,
        event_data: u64,
    ) -> Result<(), SbiRet> {
        // Fixed counters already count their assigned events and have no event selector.
        if (mhpm_offset == 0 && event_idx == 1) || (mhpm_offset == 2 && event_idx == 2) {
            return Ok(());
        }
        // Only programmable counters have configurable event selectors.
        if mhpm_offset == 1 || mhpm_offset > 31 {
            return Err(SbiRet::not_supported());
        }

        let event = EventIdx::new(event_idx);

        let mhpmevent_val = if event.is_raw_event() {
            event_data
        } else if let Some(ref event_to_mhpmevent) = self.event_to_mhpmevent {
            *event_to_mhpmevent
                .get(&(event_idx as u32))
                .ok_or(SbiRet::not_supported())?
        } else if self.event_to_mhpmcounter.is_some() {
            // QEMU can provide only counter mappings, using the event index as
            // the raw event value.
            event_idx as u64
        } else {
            return Err(SbiRet::not_supported());
        };

        write_mhpmevent(mhpm_offset, mhpmevent_val);
        Ok(())
    }

    pub fn insert_event_to_mhpmevent(&mut self, event: u32, mhpmevent: u64) {
        let event_to_mhpmevent_map = self.event_to_mhpmevent.get_or_insert_default();

        // TODO: Use `try_insert` when https://github.com/rust-lang/rust/issues/82766 is stable.
        if let Some(mhpmevent_mapped) = event_to_mhpmevent_map.get(&event) {
            error!(
                "Try to map event:0x{:08x} to mhpmevent:0x{:016x}, but the event has been mapped to mhpmevent:{}, please check the device tree file",
                event, mhpmevent, mhpmevent_mapped
            );
        } else {
            event_to_mhpmevent_map.insert(event, mhpmevent);
        }
    }

    pub fn insert_event_to_mhpmcounter(&mut self, event_to_counter: EventToCounterMap) {
        let event_to_mhpmcounter_map = self.event_to_mhpmcounter.get_or_insert_default();
        for event_to_mhpmcounter in event_to_mhpmcounter_map.iter() {
            if event_to_mhpmcounter.is_overlap(&event_to_counter) {
                error!(
                    "The mapping of event_to_mhpmcounter {:?} and {:?} overlap, please check the device tree file",
                    event_to_mhpmcounter, event_to_counter
                );
                return;
            }
        }
        event_to_mhpmcounter_map.push(event_to_counter);
    }

    pub fn insert_raw_event_to_mhpmcounter(&mut self, raw_event_to_counter: RawEventToCounterMap) {
        let raw_event_to_mhpmcounter_map = self.raw_event_to_mhpmcounter.get_or_insert_default();
        for raw_event_to_mhpmcounter in raw_event_to_mhpmcounter_map.iter() {
            if raw_event_to_mhpmcounter.is_overlap(&raw_event_to_counter) {
                error!(
                    "The mapping of raw_event_to_mhpmcounter {:?} and {:?} overlap, please check the device tree file",
                    raw_event_to_mhpmcounter, raw_event_to_counter
                );
                return;
            }
        }
        raw_event_to_mhpmcounter_map.push(raw_event_to_counter);
    }
}

/// Configures a counter to monitor an event based on the given flags.
#[inline]
fn configure_counter(
    pmu_state: &mut PmuState,
    counter_idx: usize,
    event: EventIdx,
    flags: flags::ConfigFlags,
) -> Result<(), SbiRet> {
    let auto_start = flags.contains(flags::ConfigFlags::AUTO_START);
    let clear_value = flags.contains(flags::ConfigFlags::CLEAR_VALUE);
    if event.is_firmware_event() {
        let firmware_event_idx = counter_idx - pmu_state.hw_counters_num;
        if firmware_event_idx >= PMU_FIRMWARE_COUNTER_MAX {
            return Err(SbiRet::invalid_param());
        }
        if clear_value {
            pmu_state.fw_counter[firmware_event_idx] = 0;
        }
        if auto_start {
            pmu_state.fw_counter_state |= 1 << firmware_event_idx;
        }
    } else {
        let mhpm_offset = get_mhpm_csr_offset(counter_idx).unwrap();
        if clear_value {
            write_mhpmcounter(mhpm_offset, 0);
        }
        if auto_start && start_hardware_counter(mhpm_offset, 0, false).is_err() {
            return Err(SbiRet::not_supported());
        }
    }
    Ok(())
}

/// Returns the counter's CSR offset relative to `mcycle`.
fn get_mhpm_csr_offset(counter_idx: usize) -> Option<u16> {
    let hart = HartId::current().expect("BUG: current hart exceeds Runtime capacity");
    let mask = mhpm_mask(hart);
    let mut count = 0;
    for offset in 0..32 {
        if (mask >> offset) & 1 == 1 {
            if count == counter_idx {
                return Some(offset as u16);
            }
            count += 1;
        }
    }
    None
}

/// Writes a programmable event selector for this hart.
fn write_mhpmevent(offset: u16, value: u64) {
    seq_macro::seq!(N in 3..=31 {
        match offset {
            #( N => {
                // SAFETY: The PMU owns the detected counter's event configuration.
                unsafe { machine::PerformanceEvent::<N>::write((value as usize).into()) }
            }, )*
            _ => {}
        }
    });
}

/// Writes the selected counter, with the high word first on RV32.
fn write_mhpmcounter(offset: u16, value: u64) {
    // SAFETY: The PMU owns the selected counter on this hart. Shared cycle
    // counters are not coordinated with other harts.
    unsafe {
        match offset {
            0 => {
                #[cfg(target_pointer_width = "32")]
                machine::CycleCounterHigh::write(((value >> 32) as usize).into());
                machine::CycleCounter::write((value as usize).into());
            }
            2 => {
                #[cfg(target_pointer_width = "32")]
                machine::InstructionCounterHigh::write(((value >> 32) as usize).into());
                machine::InstructionCounter::write((value as usize).into());
            }
            _ => seq_macro::seq!(N in 3..=31 {
                match offset {
                    #( N => {
                        #[cfg(target_pointer_width = "32")]
                        machine::PerformanceCounterHigh::<N>::write(((value >> 32) as usize).into());
                        machine::PerformanceCounter::<N>::write((value as usize).into());
                    }, )*
                    _ => {}
                }
            }),
        }
    }
}

/// Failure to start a performance counter.
enum StartCounterErr {
    OffsetInvalid,
    AlreadyStart,
}

fn set_counter_inhibited(counter: HardwareCounter, inhibited: bool) {
    // SAFETY: The caller coordinates this hart's selected PMU counter. Shared
    // cycle-counter control is not coordinated with other harts.
    unsafe { machine::CounterInhibit::set(counter, inhibited) };
    // Read the machine counter after changing its inhibit bit to support
    // QEMU's lower-mode counter shadows.
    match counter {
        HardwareCounter::Cycle => {
            let _ = machine::CycleCounter::read64();
        }
        HardwareCounter::InstructionsRetired => {
            let _ = machine::InstructionCounter::read64();
        }
        HardwareCounter::Hpm(_) => {}
    }
}

/// Starts a hardware performance counter specified by the offset.
fn start_hardware_counter(
    mhpm_offset: u16,
    new_value: u64,
    is_update_value: bool,
) -> Result<(), StartCounterErr> {
    let Some(counter) = HardwareCounter::from_index(mhpm_offset) else {
        return Err(StartCounterErr::OffsetInvalid);
    };

    let hart = HartId::current().expect("BUG: current hart exceeds Runtime capacity");
    if privileged_version(hart) < PrivilegedVersion::Version1_11 {
        if is_update_value {
            write_mhpmcounter(mhpm_offset, new_value);
        }
        return Ok(());
    }

    // A zero inhibit bit means the counter is running.
    if !machine::CounterInhibit::read().contains(counter) {
        return Err(StartCounterErr::AlreadyStart);
    }

    if is_update_value {
        write_mhpmcounter(mhpm_offset, new_value);
    }

    set_counter_inhibited(counter, false);
    Ok(())
}

/// Failure to stop a performance counter.
enum StopCounterErr {
    OffsetInvalid,
    AlreadyStop,
}

/// Stops a hardware performance counter specified by the offset.
fn stop_hardware_counter(mhpm_offset: u16, is_reset: bool) -> Result<(), StopCounterErr> {
    let Some(counter) = HardwareCounter::from_index(mhpm_offset) else {
        return Err(StopCounterErr::OffsetInvalid);
    };

    if is_reset && (3..=31).contains(&mhpm_offset) {
        write_mhpmevent(mhpm_offset, 0);
    }

    let hart = HartId::current().expect("BUG: current hart exceeds Runtime capacity");
    if privileged_version(hart) < PrivilegedVersion::Version1_11 {
        return Ok(());
    }

    if machine::CounterInhibit::read().contains(counter) {
        return Err(StopCounterErr::AlreadyStop);
    }

    set_counter_inhibited(counter, true);
    Ok(())
}

/// Packed SBI PMU counter information.
///
/// See <https://docs.riscv.org/reference/sbi/v3.0/ext-pmu.html>.
struct CounterInfo {
    /// Packed representation of counter information:
    ///
    /// - Bits `[11:0]`: CSR number for hardware counters.
    /// - Bits `[17:12]`: One less than the hardware counter's width in bits.
    /// - MSB: Set for firmware counters, clear for hardware counters.
    inner: usize,
}

#[allow(unused)]
impl CounterInfo {
    const CSR_MASK: usize = 0xFFF; // Bits [11:0]
    const WIDTH_MASK: usize = 0x3F << 12; // Bits [17:12]
    const FIRMWARE_FLAG: usize = 1 << (usize::BITS as usize - 1); // MSB

    #[inline]
    pub const fn new() -> Self {
        Self { inner: 0 }
    }

    #[inline]
    pub fn set_csr(&mut self, csr_num: u16) {
        self.inner = (self.inner & !Self::CSR_MASK) | ((csr_num as usize) & Self::CSR_MASK);
    }

    #[inline]
    pub fn set_width(&mut self, width: u8) {
        self.inner = (self.inner & !Self::WIDTH_MASK) | (((width as usize) & 0x3F) << 12);
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

impl Default for CounterInfo {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Copy)]
pub struct EventIdx {
    /// Packed representation of event information:
    ///
    /// - Bits `[15:0]`: Event code.
    /// - Bits `[19:16]`: Event type.
    inner: usize,
}

#[allow(unused)]
impl EventIdx {
    #[inline]
    const fn new(event_idx: usize) -> Self {
        Self { inner: event_idx }
    }

    #[inline]
    fn from_firmware_event(firmware_event: usize) -> Self {
        Self {
            inner: 0xf << 16 | firmware_event,
        }
    }

    #[inline]
    fn raw(&self) -> usize {
        self.inner
    }

    #[inline]
    const fn event_type(&self) -> usize {
        (self.inner >> 16) & 0xF
    }

    #[inline]
    const fn event_code(&self) -> usize {
        self.inner & 0xFFFF
    }

    /// Returns the cache ID for `HARDWARE_CACHE` events (bits `[15:3]`).
    #[inline]
    const fn cache_id(&self) -> usize {
        (self.inner >> 3) & 0x1FFF
    }

    /// Returns the cache operation ID (bits `[2:1]`).
    #[inline]
    const fn cache_op_id(&self) -> usize {
        (self.inner >> 1) & 0x3
    }

    /// Returns the cache result ID (bit `[0]`).
    #[inline]
    const fn cache_result_id(&self) -> usize {
        self.inner & 0x1
    }

    #[inline]
    const fn is_general_event(&self) -> bool {
        self.event_type() == event_type::HARDWARE_GENERAL
    }

    #[inline]
    const fn is_cache_event(&self) -> bool {
        self.event_type() == event_type::HARDWARE_CACHE
    }

    #[inline]
    const fn is_raw_event_v1(&self) -> bool {
        self.event_type() == event_type::HARDWARE_RAW
    }

    #[inline]
    const fn is_raw_event_v2(&self) -> bool {
        self.event_type() == event_type::HARDWARE_RAW_V2
    }

    #[inline]
    const fn is_raw_event(&self) -> bool {
        self.is_raw_event_v1() || self.is_raw_event_v2()
    }

    #[inline]
    const fn is_firmware_event(&self) -> bool {
        self.event_type() == event_type::FIRMWARE
    }

    #[inline]
    fn check_event_type(self) -> bool {
        let event_type = self.event_type();
        let event_code = self.event_code();

        match event_type {
            event_type::HARDWARE_GENERAL => event_code <= hardware_event::REF_CPU_CYCLES,
            event_type::HARDWARE_CACHE => {
                self.cache_id() <= cache_event::NODE
                    && self.cache_op_id() <= cache_operation::PREFETCH
                    && self.cache_result_id() <= cache_result::MISS
            }
            event_type::HARDWARE_RAW | event_type::HARDWARE_RAW_V2 => event_code == 0,
            event_type::FIRMWARE => true,
            _ => false,
        }
    }

    #[inline]
    fn firmware_event_valid(self) -> bool {
        let event_type = self.event_type();
        let event_code = self.event_code();
        if event_type != event_type::FIRMWARE {
            return false;
        }
        if (event_code > firmware_event::HFENCE_VVMA_ASID_RECEIVED
            && event_code < firmware_event::PLATFORM)
            || event_code >= firmware_event::PLATFORM
        {
            // Platform-specific firmware events are not implemented.
            return false;
        }
        true
    }
}

/// A mapping from an event-code range to supported hardware counters.
#[derive(Debug)]
pub struct EventToCounterMap {
    counters_mask: u32,   // Bitmask of supported counters
    event_start_idx: u32, // Start of event code range
    event_end_idx: u32,   // End of event code range
}

impl EventToCounterMap {
    pub fn new(counters_mask: u32, event_start_idx: u32, event_end_idx: u32) -> Self {
        Self {
            counters_mask,
            event_start_idx,
            event_end_idx,
        }
    }

    #[inline]
    pub const fn have_event(&self, event_idx: u32) -> bool {
        event_idx >= self.event_start_idx && event_idx <= self.event_end_idx
    }

    #[inline]
    pub fn get_counter_mask(&self) -> u32 {
        self.counters_mask
    }

    #[inline]
    pub fn is_overlap(&self, other_map: &EventToCounterMap) -> bool {
        if (self.event_end_idx < other_map.event_start_idx
            && self.event_end_idx < other_map.event_end_idx)
            || (self.event_start_idx > other_map.event_start_idx
                && self.event_start_idx > other_map.event_end_idx)
        {
            return false;
        }
        true
    }
}

#[derive(Debug)]
pub struct RawEventToCounterMap {
    counters_mask: u32,    // Bitmask of supported counters
    raw_event_select: u64, // Value to program into mhpmeventX
    select_mask: u64,      // Mask for selecting bits (optional use)
}

impl RawEventToCounterMap {
    pub fn new(counters_mask: u32, raw_event_select: u64, select_mask: u64) -> Self {
        Self {
            counters_mask,
            raw_event_select,
            select_mask,
        }
    }

    #[inline]
    pub const fn have_event(&self, event_idx: u64) -> bool {
        self.raw_event_select == (event_idx & self.select_mask)
    }

    #[inline]
    pub const fn get_counter_mask(&self) -> u32 {
        self.counters_mask
    }

    #[inline]
    pub const fn is_overlap(&self, other_map: &RawEventToCounterMap) -> bool {
        self.select_mask == other_map.select_mask
            && self.raw_event_select == other_map.raw_event_select
    }
}

struct CounterMask {
    counter_idx_base: usize,
    counter_idx_mask: usize,
}

impl CounterMask {
    pub fn new(counter_idx_base: usize, counter_idx_mask: usize) -> Self {
        Self {
            counter_idx_base,
            counter_idx_mask,
        }
    }
}

impl Iterator for CounterMask {
    type Item = usize;

    fn next(&mut self) -> Option<Self::Item> {
        if self.counter_idx_mask == 0 {
            None
        } else {
            let low_bit = self.counter_idx_mask.trailing_zeros();
            let hart_id = usize::try_from(low_bit).unwrap() + self.counter_idx_base;
            self.counter_idx_mask &= !(1usize << low_bit);
            Some(hart_id)
        }
    }
}

pub fn pmu_firmware_counter_increment(firmware_event: usize) {
    if super::pmu().is_none() {
        return;
    }
    let mut pmu_state = current_state().lock();
    // Most SBI calls run without firmware event counters enabled.
    if pmu_state.fw_counter_state == 0 {
        return;
    }
    let counter_idx_start = pmu_state.hw_counters_num;
    for counter_idx in counter_idx_start..counter_idx_start + PMU_FIRMWARE_COUNTER_MAX {
        let fw_idx = counter_idx - counter_idx_start;
        if pmu_state.active_event[counter_idx]
            == EventIdx::from_firmware_event(firmware_event).raw()
            && pmu_state.is_firmware_event_start(counter_idx)
        {
            pmu_state.fw_counter[fw_idx] += 1;
        }
    }
}

/// Reads the event mappings of the PMU selected during platform discovery.
pub(crate) fn from_node(pmu: FdtNode<'_, '_>) -> Option<SbiPmu> {
    let mut sbi_pmu = SbiPmu::default();
    if let Some(property) = pmu.property("riscv,event-to-mhpmevent") {
        let rows = property_rows::<3>(property.value)?;
        for row in rows {
            let event = row[0];
            let mhpmevent = (u64::from(row[1]) << 32) | u64::from(row[2]);
            sbi_pmu.insert_event_to_mhpmevent(event, mhpmevent);
            debug!(
                "pmu: insert event: 0x{:08x}, mhpmevent: {:#016x}",
                event, mhpmevent
            );
        }
    }

    if let Some(property) = pmu.property("riscv,event-to-mhpmcounters") {
        let rows = property_rows::<3>(property.value)?;
        for row in rows {
            let event_to_counter = EventToCounterMap::new(row[2], row[0], row[1]);
            debug!("pmu: insert event_to_mhpmcounter: {:x?}", event_to_counter);
            sbi_pmu.insert_event_to_mhpmcounter(event_to_counter);
        }
    }

    if let Some(property) = pmu.property("riscv,raw-event-to-mhpmcounters") {
        let rows = property_rows::<5>(property.value)?;
        for row in rows {
            let raw_event_select = (u64::from(row[0]) << 32) | u64::from(row[1]);
            let select_mask = (u64::from(row[2]) << 32) | u64::from(row[3]);
            let counters_mask = row[4];
            let raw_event_to_counter =
                RawEventToCounterMap::new(counters_mask, raw_event_select, select_mask);
            debug!(
                "pmu: insert raw_event_to_mhpmcounter: {:x?}",
                raw_event_to_counter
            );
            sbi_pmu.insert_raw_event_to_mhpmcounter(raw_event_to_counter);
        }
    }
    Some(sbi_pmu)
}

fn property_rows<const COLUMNS: usize>(
    bytes: &[u8],
) -> Option<impl Iterator<Item = [u32; COLUMNS]> + '_> {
    let row_size = COLUMNS.checked_mul(core::mem::size_of::<u32>())?;
    let rows = bytes.chunks_exact(row_size);
    rows.remainder().is_empty().then_some(rows.map(|row| {
        core::array::from_fn(|column| {
            let offset = column * core::mem::size_of::<u32>();
            u32::from_be_bytes(row[offset..offset + 4].try_into().unwrap())
        })
    }))
}
