//! Performance monitoring.
//!
//! # References
//!
//! - Specification: [RISC-V SBI PMU extension](https://docs.riscv.org/reference/sbi/v3.0/ext-pmu.html) —
//!   counter discovery, event configuration, and shared-memory operations.

use alloc::collections::{BTreeMap, btree_map::Entry};
use alloc::vec::Vec;
use runtime::{
    FdtNode, Result,
    events::{Exception, TrapCounters},
    hart::{HartId, PerHart, Xlen},
    pmu::{self, HardwareCounter},
    rustsbi::{Pmu, SbiRet},
};
use sbi_spec::binary::CounterMask;
use sbi_spec::pmu::*;
use spin::{Mutex, Once};

use super::features::{PrivilegedVersion, privileged_version, supervisor_xlen};

/// Maximum number of hardware performance counters supported.
const PMU_HARDWARE_COUNTER_MAX: usize = 32;
/// Maximum number of firmware-managed counters supported.
const PMU_FIRMWARE_COUNTER_MAX: usize = 16;
/// Marker value for inactive/invalid event indices.
const PMU_EVENT_IDX_INVALID: usize = usize::MAX;

/// Per-hart PMU records, published before platform discovery.
static PMU_STATES: Once<PerHart<Mutex<PmuState>>> = Once::new();

// A per-hart lock cannot serialize writes to mcycle or mcountinhibit.CY
// on cores that share the cycle counter.
static CYCLE_CONTROL: Mutex<()> = Mutex::new(());

fn lock_cycle_if_selected(base: usize, mask: usize) -> Option<spin::MutexGuard<'static, ()>> {
    (base == 0 && mask & 1 != 0).then(|| CYCLE_CONTROL.lock())
}

/// Creates one PMU record per enabled hart on the boot hart.
///
/// The caller must run this after Runtime publishes the hart topology and
/// before any SBI service observes PMU state.
pub(crate) fn init() {
    PMU_STATES.call_once(|| {
        PerHart::new(|_| Mutex::new(PmuState::new([0; PMU_HARDWARE_COUNTER_MAX])))
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

pub(super) fn hardware_counter_mask(hart: HartId) -> u32 {
    state(hart).lock().mhpm_mask
}

/// Rebuilds the current hart's PMU state using its detected counter widths.
/// `hart` must identify the current hart.
pub(crate) fn init_current(hart: HartId, widths: [u8; PMU_HARDWARE_COUNTER_MAX]) {
    state(hart).lock().reset(widths);
}

/// Resets the current hart's PMU state before a non-retentive resume.
///
/// Pending IPIs and fence requests remain valid across suspend/resume.
pub(crate) fn reset_current() {
    let hart = HartId::current().expect("BUG: current hart is not in the boot topology");
    if privileged_version(hart) >= PrivilegedVersion::Version1_11 {
        // SAFETY: Resume owns the programmable counters before returning to S-mode.
        unsafe { pmu::inhibit_programmable() };
    }
    let mut state = state(hart).lock();
    let widths = state.widths;
    state.reset(widths);
}

/// PMU state for hardware and firmware performance counters.
#[repr(C)]
pub struct PmuState {
    active_event: [usize; PMU_HARDWARE_COUNTER_MAX + PMU_FIRMWARE_COUNTER_MAX],
    /// Bitmap of active firmware counters, with one bit per counter.
    fw_counter_state: usize,
    /// Values of firmware-managed counters.
    fw_counter: [u64; PMU_FIRMWARE_COUNTER_MAX],
    /// Advertised hardware counters, one bit per CSR offset.
    mhpm_mask: u32,
    widths: [u8; PMU_HARDWARE_COUNTER_MAX],
    /// Counters whose event selectors must be reprogrammed before they can start.
    event_config_failed: u32,
    hw_counters_num: usize,
}

impl PmuState {
    fn reset(&mut self, widths: [u8; PMU_HARDWARE_COUNTER_MAX]) {
        let mut next = Self::new(widths);
        next.event_config_failed = self.event_config_failed & next.mhpm_mask;
        *self = next;
    }

    fn new(widths: [u8; PMU_HARDWARE_COUNTER_MAX]) -> Self {
        let mhpm_mask = widths.iter().enumerate().fold(0, |mask, (idx, &width)| {
            mask | (u32::from(width != 0) << idx)
        });
        // Keep gaps in the CSR address range so hardware indices match the
        // device-tree counter bitmap. The first three slots are architectural.
        let hw_counters_num = (32 - mhpm_mask.leading_zeros() as usize).max(3);

        let mut active_event =
            [PMU_EVENT_IDX_INVALID; PMU_HARDWARE_COUNTER_MAX + PMU_FIRMWARE_COUNTER_MAX];
        // Fixed counters have architectural event assignments.
        active_event[0] = hardware_event::CPU_CYCLES;
        active_event[2] = hardware_event::INSTRUCTIONS;

        Self {
            active_event,
            fw_counter_state: 0,
            fw_counter: [0; PMU_FIRMWARE_COUNTER_MAX],
            mhpm_mask,
            widths,
            event_config_failed: 0,
            hw_counters_num,
        }
    }

    /// Returns the first firmware counter index.
    ///
    /// Lower indices can correspond to unimplemented hardware counters.
    #[inline(always)]
    pub fn get_hw_counter_num(&self) -> usize {
        self.hw_counters_num
    }

    /// Returns the upper bound on logical counter indices, including hardware gaps.
    #[inline(always)]
    pub fn get_total_counters_num(&self) -> usize {
        self.hw_counters_num + PMU_FIRMWARE_COUNTER_MAX
    }

    /// Returns the implemented hardware counter selected by its logical index.
    ///
    /// Prototyper assigns indices by CSR offset from `mcycle`.
    /// Index 1 corresponds to `time`, which has no machine counter.
    #[inline]
    fn hardware_counter(&self, counter_idx: usize) -> Option<HardwareCounter> {
        if counter_idx >= PMU_HARDWARE_COUNTER_MAX || self.widths[counter_idx] == 0 {
            return None;
        }
        HardwareCounter::new(counter_idx as u16)
    }

    fn counter_indices(
        &self,
        mask: CounterMask,
    ) -> Option<impl Iterator<Item = usize> + Clone + use<>> {
        let (mut bits, base) = mask.into_inner();
        let last = base.checked_add(bits.checked_ilog2()? as usize)?;
        if last >= self.get_total_counters_num() {
            return None;
        }
        let indices = core::iter::from_fn(move || {
            if bits == 0 {
                return None;
            }
            let index = base + bits.trailing_zeros() as usize;
            bits &= bits - 1;
            Some(index)
        });
        // Reject a mask containing a hardware gap before any earlier counter changes.
        indices
            .clone()
            .all(|idx| idx >= self.hw_counters_num || self.hardware_counter(idx).is_some())
            .then_some(indices)
    }

    /// Returns the value of a firmware counter.
    #[inline]
    pub fn get_fw_counter(&self, counter_idx: usize) -> Option<u64> {
        let fw_idx = counter_idx.checked_sub(self.hw_counters_num)?;
        self.fw_counter.get(fw_idx).copied()
    }

    /// Starts a firmware counter, optionally setting its initial value.
    #[inline]
    fn start_fw_counter(
        &mut self,
        counter_idx: usize,
        initial_value: u64,
        is_update_value: bool,
    ) -> Result<(), StartCounterErr> {
        if counter_idx < self.hw_counters_num || counter_idx >= self.get_total_counters_num() {
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
        if counter_idx < self.hw_counters_num || counter_idx >= self.get_total_counters_num() {
            return Err(StopCounterErr::OffsetInvalid);
        }
        let fw_idx = counter_idx - self.hw_counters_num;

        if is_reset {
            self.active_event[counter_idx] = PMU_EVENT_IDX_INVALID;
        }
        if self.fw_counter_state & (1 << fw_idx) == 0 {
            return Err(StopCounterErr::AlreadyStop);
        }
        self.fw_counter_state &= !(1 << fw_idx);
        Ok(())
    }

    #[inline]
    pub fn is_firmware_event_start(&self, counter_idx: usize) -> bool {
        if counter_idx < self.hw_counters_num || counter_idx >= self.get_total_counters_num() {
            return false;
        }
        let fw_idx = counter_idx - self.hw_counters_num;
        self.fw_counter_state & (1 << fw_idx) != 0
    }

    fn stop_hw_counter(
        &mut self,
        counter: HardwareCounter,
        reset: bool,
    ) -> Result<(), StopCounterErr> {
        let index = usize::from(counter.index());
        let mask = 1u32 << index;
        let hart = HartId::current().expect("BUG: current hart exceeds Runtime capacity");
        let can_inhibit = privileged_version(hart) >= PrivilegedVersion::Version1_11;
        let already_stopped = can_inhibit && pmu::is_inhibited(counter);
        if can_inhibit && !already_stopped {
            set_counter_inhibited(counter, true);
        }
        // Fixed counters retain their architectural event assignments.
        if reset && let HardwareCounter::Hpm(hpm) = counter {
            self.active_event[index] = PMU_EVENT_IDX_INVALID;
            // SAFETY: This hart owns the selector. Runtime confirms inhibition
            // before updating a paired RV32 selector.
            if let Err(error) = unsafe { pmu::write_event(hpm, 0) } {
                self.event_config_failed |= mask;
                return Err(StopCounterErr::Hardware(error));
            }
            self.event_config_failed &= !mask;
        }
        if already_stopped {
            Err(StopCounterErr::AlreadyStop)
        } else {
            Ok(())
        }
    }
}

#[derive(Default)]
pub struct SbiPmu {
    event_to_mhpmevent: BTreeMap<u32, u64>,
    event_to_mhpmcounter: Vec<EventToCounterMap>,
    raw_event_to_mhpmcounter: Vec<RawEventToCounterMap>,
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

    fn record_supervisor_ipi(&self) {
        pmu_firmware_counter_increment(firmware_event::IPI_RECEIVED);
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
        let info = if counter_idx < pmu_state.get_hw_counter_num() {
            let Some(counter) = pmu_state.hardware_counter(counter_idx) else {
                return SbiRet::invalid_param();
            };
            CounterInfo::hardware(counter.user_csr(), pmu_state.widths[counter_idx])
        } else {
            CounterInfo::firmware()
        };
        match supervisor_xlen(hart) {
            Some(Xlen::X32) => SbiRet::success(info.bits32() as usize),
            Some(Xlen::X64) => SbiRet::success(info.bits64() as usize),
            None => SbiRet::failed(),
        }
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

        let event = EventIdx::from_bits(event_idx);
        let mut pmu_state = current_state().lock();
        let _cycle = lock_cycle_if_selected(counter_idx_base, counter_idx_mask);
        let mask = CounterMask::from_mask_base(counter_idx_mask, counter_idx_base);
        let Some(mut counters) = pmu_state.counter_indices(mask) else {
            return SbiRet::invalid_param();
        };
        if !event_valid(event) {
            return SbiRet::invalid_param();
        }

        let counter_idx = if flags.contains(flags::ConfigFlags::SKIP_MATCH) {
            let index = counters.next().expect("validated nonempty counter mask");
            if event.is_firmware() != (index >= pmu_state.hw_counters_num) {
                return SbiRet::not_supported();
            }
            if pmu_state.active_event[index] == PMU_EVENT_IDX_INVALID {
                return SbiRet::invalid_param();
            }
            index
        } else {
            let result = if event.is_firmware() {
                self.find_firmware_counter(counters, &pmu_state)
            } else {
                self.find_hardware_counter(counters, event, event_data, &mut pmu_state)
            };
            let index = match result {
                Ok(index) => index,
                Err(error) => return error,
            };
            pmu_state.active_event[index] = event_idx;
            index
        };

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
        let _cycle = lock_cycle_if_selected(counter_idx_base, counter_idx_mask);
        let is_update_value = flags.contains(flags::StartFlags::INIT_VALUE);

        let mask = CounterMask::from_mask_base(counter_idx_mask, counter_idx_base);
        let Some(counters) = pmu_state.counter_indices(mask) else {
            return SbiRet::invalid_param();
        };

        if flags.contains(flags::StartFlags::INIT_SNAPSHOT) {
            return SbiRet::no_shmem();
        }

        for counter_idx in counters {
            if counter_idx < pmu_state.hw_counters_num
                && pmu_state.event_config_failed & (1u32 << counter_idx) != 0
            {
                return SbiRet::failed();
            }
            let start_result = if counter_idx >= pmu_state.get_hw_counter_num() {
                pmu_state.start_fw_counter(counter_idx, initial_value, is_update_value)
            } else {
                match pmu_state.hardware_counter(counter_idx) {
                    Some(counter) => {
                        start_hardware_counter(counter, initial_value, is_update_value)
                    }
                    None => Err(StartCounterErr::OffsetInvalid),
                }
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
        let _cycle = lock_cycle_if_selected(counter_idx_base, counter_idx_mask);
        let is_reset = flags.contains(flags::StopFlags::RESET);

        let mask = CounterMask::from_mask_base(counter_idx_mask, counter_idx_base);
        let Some(counters) = pmu_state.counter_indices(mask) else {
            return SbiRet::invalid_param();
        };

        if flags.contains(flags::StopFlags::TAKE_SNAPSHOT) {
            return SbiRet::no_shmem();
        }

        let mut result = SbiRet::success(0);
        for counter_idx in counters {
            let stop_result = if counter_idx >= pmu_state.get_hw_counter_num() {
                pmu_state.stop_fw_counter(counter_idx, is_reset)
            } else {
                let Some(counter) = pmu_state.hardware_counter(counter_idx) else {
                    return SbiRet::invalid_param();
                };
                pmu_state.stop_hw_counter(counter, is_reset)
            };
            match stop_result {
                Ok(()) => {}
                Err(StopCounterErr::OffsetInvalid) => return SbiRet::invalid_param(),
                Err(StopCounterErr::AlreadyStop) => result = SbiRet::already_stopped(),
                Err(StopCounterErr::Hardware(error)) => return error.into(),
            }
        }
        result
    }

    /// Reads a firmware counter (SBI PMU function 5).
    #[inline]
    fn counter_fw_read(&self, counter_idx: usize) -> SbiRet {
        match current_state().lock().get_fw_counter(counter_idx) {
            Some(value) => SbiRet::success(value as usize),
            None => SbiRet::invalid_param(),
        }
    }

    /// Reads a firmware counter's high bits (SBI PMU function 6).
    #[inline]
    fn counter_fw_read_hi(&self, counter_idx: usize) -> SbiRet {
        let hart = HartId::current().expect("BUG: current hart exceeds Runtime capacity");
        let Some(value) = state(hart).lock().get_fw_counter(counter_idx) else {
            return SbiRet::invalid_param();
        };
        match supervisor_xlen(hart) {
            Some(Xlen::X32) => SbiRet::success((value >> 32) as usize),
            Some(Xlen::X64) => SbiRet::success(0),
            None => SbiRet::failed(),
        }
    }
}

impl SbiPmu {
    fn find_firmware_counter(
        &self,
        mut counters: impl Iterator<Item = usize>,
        pmu_state: &PmuState,
    ) -> Result<usize, SbiRet> {
        counters
            .find(|&index| {
                index >= pmu_state.hw_counters_num
                    && pmu_state.active_event[index] == PMU_EVENT_IDX_INVALID
            })
            .ok_or_else(SbiRet::not_supported)
    }

    fn find_hardware_counter(
        &self,
        counters: impl Iterator<Item = usize> + Clone,
        event: EventIdx,
        event_data: u64,
        pmu_state: &mut PmuState,
    ) -> Result<usize, SbiRet> {
        // Fixed counters have architectural event assignments even when
        // the device tree provides no programmable-counter mapping.
        let fixed_counter = counters.clone().find(|&idx| {
            matches!(
                (event.bits(), pmu_state.hardware_counter(idx)),
                (hardware_event::CPU_CYCLES, Some(HardwareCounter::Cycle))
                    | (
                        hardware_event::INSTRUCTIONS,
                        Some(HardwareCounter::InstructionsRetired)
                    )
            )
        });
        let mask = if event.is_raw() {
            self.raw_event_to_mhpmcounter
                .iter()
                .find(|mapping| mapping.have_event(event_data))
                .map(RawEventToCounterMap::get_counter_mask)
        } else {
            self.event_to_mhpmcounter
                .iter()
                .find(|mapping| mapping.have_event(event.bits() as u32))
                .map(EventToCounterMap::get_counter_mask)
        };
        let Some(mask) = mask else {
            return fixed_counter.ok_or_else(SbiRet::not_supported);
        };
        let hart = HartId::current().expect("BUG: current hart exceeds Runtime capacity");
        let privileged_version = privileged_version(hart);

        for counter_idx in counters {
            let Some(HardwareCounter::Hpm(counter)) = pmu_state.hardware_counter(counter_idx)
            else {
                continue;
            };
            if mask & (1u32 << counter_idx) == 0
                || pmu_state.active_event[counter_idx] != PMU_EVENT_IDX_INVALID
            {
                continue;
            }
            if privileged_version >= PrivilegedVersion::Version1_11
                && !pmu::is_inhibited(HardwareCounter::Hpm(counter))
            {
                continue;
            }

            let value = self.hardware_event_value(event, event_data)?;
            // SAFETY: Matching owns this detected counter. Runtime verifies
            // stopping before a paired update and owns the recovery window.
            if let Err(error) = unsafe { pmu::write_event(counter, value) } {
                pmu_state.event_config_failed |= 1u32 << counter_idx;
                return Err(SbiRet::from(error));
            }
            pmu_state.event_config_failed &= !(1u32 << counter_idx);
            return Ok(counter_idx);
        }
        fixed_counter.ok_or_else(SbiRet::not_supported)
    }

    fn hardware_event_value(&self, event: EventIdx, event_data: u64) -> Result<u64, SbiRet> {
        if event.is_raw() {
            // SBI leaves the upper 16 bits (raw) or 8 bits (raw v2) of a
            // 64-bit selector under firmware control.
            let mask = if event.event_type() == event_type::HARDWARE_RAW {
                u64::MAX >> 16
            } else {
                u64::MAX >> 8
            };
            Ok(event_data & mask)
        } else if self.event_to_mhpmevent.is_empty() {
            // QEMU can provide only a counter mapping, using the event index as the selector.
            Ok(event.bits() as u64)
        } else {
            self.event_to_mhpmevent
                .get(&(event.bits() as u32))
                .copied()
                .ok_or_else(SbiRet::not_supported)
        }
    }

    pub fn insert_event_to_mhpmevent(&mut self, event: u32, mhpmevent: u64) {
        match self.event_to_mhpmevent.entry(event) {
            Entry::Occupied(entry) => error!(
                "Try to map event:0x{:08x} to mhpmevent:0x{:016x}, but the event has been mapped to mhpmevent:{}, please check the device tree file",
                event,
                mhpmevent,
                entry.get()
            ),
            Entry::Vacant(entry) => {
                entry.insert(mhpmevent);
            }
        }
    }

    pub fn insert_event_to_mhpmcounter(&mut self, event_to_counter: EventToCounterMap) {
        for event_to_mhpmcounter in &self.event_to_mhpmcounter {
            if event_to_mhpmcounter.is_overlap(&event_to_counter) {
                error!(
                    "The mapping of event_to_mhpmcounter {:?} and {:?} overlap, please check the device tree file",
                    event_to_mhpmcounter, event_to_counter
                );
                return;
            }
        }
        self.event_to_mhpmcounter.push(event_to_counter);
    }

    pub fn insert_raw_event_to_mhpmcounter(&mut self, raw_event_to_counter: RawEventToCounterMap) {
        for raw_event_to_mhpmcounter in &self.raw_event_to_mhpmcounter {
            if raw_event_to_mhpmcounter.is_overlap(&raw_event_to_counter) {
                error!(
                    "The mapping of raw_event_to_mhpmcounter {:?} and {:?} overlap, please check the device tree file",
                    raw_event_to_mhpmcounter, raw_event_to_counter
                );
                return;
            }
        }
        self.raw_event_to_mhpmcounter.push(raw_event_to_counter);
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
    if event.is_firmware() {
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
        let Some(counter) = pmu_state.hardware_counter(counter_idx) else {
            return Err(SbiRet::invalid_param());
        };
        if clear_value {
            write_mhpmcounter(counter, 0);
        }
        if auto_start && start_hardware_counter(counter, 0, false).is_err() {
            return Err(SbiRet::not_supported());
        }
    }
    Ok(())
}

fn write_mhpmcounter(counter: HardwareCounter, value: u64) {
    // SAFETY: The hart's PMU lock protects its counter. Any SBI request
    // selecting mcycle also holds CYCLE_CONTROL across the whole operation.
    unsafe { pmu::write_counter(counter, value) };
}

/// Failure to start a performance counter.
enum StartCounterErr {
    OffsetInvalid,
    AlreadyStart,
}

fn set_counter_inhibited(counter: HardwareCounter, inhibited: bool) {
    // SAFETY: The PMU lock covers this hart and CYCLE_CONTROL serializes CY
    // updates with requests on harts that may share the cycle counter.
    unsafe { pmu::set_inhibited(counter, inhibited) };
}

fn start_hardware_counter(
    counter: HardwareCounter,
    new_value: u64,
    is_update_value: bool,
) -> Result<(), StartCounterErr> {
    let hart = HartId::current().expect("BUG: current hart exceeds Runtime capacity");
    if privileged_version(hart) < PrivilegedVersion::Version1_11 {
        if is_update_value {
            write_mhpmcounter(counter, new_value);
        }
        return Ok(());
    }

    if !pmu::is_inhibited(counter) {
        return Err(StartCounterErr::AlreadyStart);
    }

    if is_update_value {
        write_mhpmcounter(counter, new_value);
    }

    set_counter_inhibited(counter, false);
    Ok(())
}

/// Failure to stop a performance counter.
enum StopCounterErr {
    OffsetInvalid,
    AlreadyStop,
    Hardware(runtime::Error),
}

#[inline]
const fn event_valid(event: EventIdx) -> bool {
    if event.bits() >> 20 != 0 {
        return false;
    }
    let event_type = event.event_type();
    let event_code = event.event_code();

    match event_type {
        event_type::HARDWARE_GENERAL => {
            event_code >= hardware_event::CPU_CYCLES && event_code <= hardware_event::REF_CPU_CYCLES
        }
        event_type::HARDWARE_CACHE => {
            event.cache_id() <= cache_event::NODE
                && event.cache_op_id() <= cache_operation::PREFETCH
        }
        event_type::HARDWARE_RAW | event_type::HARDWARE_RAW_V2 => event_code == 0,
        event_type::FIRMWARE => event_code <= firmware_event::HFENCE_VVMA_ASID_RECEIVED,
        _ => false,
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

pub fn pmu_firmware_counter_increment(firmware_event: usize) {
    if super::pmu().is_none() {
        return;
    }
    let mut pmu_state = current_state().lock();
    // Most SBI calls run without firmware event counters enabled.
    if pmu_state.fw_counter_state == 0 {
        return;
    }
    let event = EventIdx::firmware(firmware_event).bits();
    for fw_idx in 0..PMU_FIRMWARE_COUNTER_MAX {
        let counter_idx = pmu_state.hw_counters_num + fw_idx;
        if pmu_state.active_event[counter_idx] == event
            && pmu_state.is_firmware_event_start(counter_idx)
        {
            pmu_state.fw_counter[fw_idx] = pmu_state.fw_counter[fw_idx].wrapping_add(1);
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
