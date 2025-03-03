use rustsbi::{Pmu, SbiRet};
use sbi_spec::binary::SharedPtr;
use sbi_spec::pmu::shmem_size::SIZE;
use sbi_spec::pmu::*;

use crate::riscv::csr::CSR_CYCLE;
use crate::{riscv::current_hartid, sbi::features::hart_mhpm_mask};

use super::trap_stack::{hart_context, hart_context_mut};

const HARDWARE_COUNTER_MAX: usize = 32;
const FIRMWARE_COUNTER_MAX: usize = 16;

/// PMU activation event and firmware counters
pub struct PmuState {
    active_event: [usize; HARDWARE_COUNTER_MAX + FIRMWARE_COUNTER_MAX],
    // Firmware counter status mask, each bit represents a firmware counter.
    // A bit set to 1 indicates that the corresponding firmware counter starts counting
    fw_counter_state: usize,
    fw_counter: [u64; FIRMWARE_COUNTER_MAX],
    hw_counters_num: usize,
    total_counters_num: usize,
}

impl PmuState {
    pub fn new() -> Self {
        let mhpm_mask = hart_mhpm_mask(current_hartid());
        let hw_counters_num = mhpm_mask.count_ones() as usize;
        let total_counters_num = hw_counters_num + FIRMWARE_COUNTER_MAX;

        let active_event = [0; HARDWARE_COUNTER_MAX + FIRMWARE_COUNTER_MAX];

        Self {
            active_event,
            fw_counter_state: 0,
            fw_counter: [0; FIRMWARE_COUNTER_MAX],
            hw_counters_num,
            total_counters_num,
        }
    }

    #[inline]
    pub fn get_event_idx(&self, counter_idx: usize, firmware_event: bool) -> Option<EventIdx> {
        if counter_idx >= self.total_counters_num {
            return None;
        }
        if firmware_event && counter_idx < self.hw_counters_num {
            return None;
        }
        // Safety: counter_idx is checked against total_counters_num
        unsafe { Some(EventIdx::new(*self.active_event.get_unchecked(counter_idx))) }
    }

    #[inline]
    pub fn get_fw_counter(&self, counter_idx: usize) -> Option<u64> {
        if counter_idx < self.hw_counters_num || counter_idx >= self.total_counters_num {
            return None;
        }
        let fw_idx = counter_idx - self.hw_counters_num;
        // Safety: fw_idx is guaranteed to be within bounds (0..FIRMWARE_COUNTER_MAX)
        unsafe { Some(*self.fw_counter.get_unchecked(fw_idx)) }
    }

    #[inline]
    pub fn update_fw_counter(
        &mut self,
        counter_idx: usize,
        value: u64,
    ) -> Result<(), &'static str> {
        if counter_idx < self.hw_counters_num || counter_idx >= self.total_counters_num {
            return Err("Invalid counter index");
        }
        let fw_idx = counter_idx - self.hw_counters_num;
        self.fw_counter[fw_idx] = value;
        self.fw_counter_state |= 1 << fw_idx; // Mark as active
        Ok(())
    }
}

struct SbiPmu;

impl Pmu for SbiPmu {
    #[inline]
    fn num_counters(&self) -> usize {
        hart_context(current_hartid()).pmu_state.total_counters_num
    }

    #[inline]
    fn counter_get_info(&self, counter_idx: usize) -> SbiRet {
        if counter_idx >= self.num_counters() {
            return SbiRet::invalid_param();
        }

        let pmu_state = &hart_context(current_hartid()).pmu_state;
        if counter_idx < pmu_state.hw_counters_num {
            let mut mask = hart_mhpm_mask(current_hartid());
            let mut count = 0;
            while mask != 0 {
                if count == counter_idx {
                    let offset = mask.trailing_zeros() as u16;
                    return SbiRet::success(
                        CounterInfo::with_hardware_info(CSR_CYCLE + offset, 63).inner(),
                    );
                }
                mask &= mask - 1;
                count += 1;
            }
            return SbiRet::invalid_param();
        }

        SbiRet::success(CounterInfo::with_firmware_info().inner())
    }

    /// Find and configure a matching counter (FID #2)
    #[inline]
    fn counter_config_matching(
        &self,
        counter_idx_base: usize,
        counter_idx_mask: usize,
        config_flags: usize,
        event_idx: usize,
        event_data: u64,
    ) -> SbiRet {
        let flags = match flags::CounterCfgFlags::from_bits(config_flags) {
            Some(flags) => flags,
            None => return SbiRet::invalid_param(), // Reserved bits are set
        };

        let event = EventIdx::new(event_idx);
        let pmu_state = &mut hart_context_mut(current_hartid()).pmu_state;
        let is_firmware_event = event.event_type() == event_type::FIRMWARE;

        if counter_idx_base >= pmu_state.total_counters_num
            || (counter_idx_mask & ((1 << pmu_state.total_counters_num) - 1)) == 0
            || !event.check_event_type()
            || (is_firmware_event && !event.is_firmware_event_valid())
        {
            return SbiRet::invalid_param();
        }

        let effective_mask = counter_idx_mask & ((1 << pmu_state.total_counters_num) - 1);
        let max_counters = pmu_state
            .total_counters_num
            .saturating_sub(counter_idx_base);
        let skip_match = flags.contains(flags::CounterCfgFlags::SKIP_MATCH);

        // Unified counter selection and configuration
        for i in 0..max_counters {
            let counter_idx = counter_idx_base + i;
            if effective_mask & (1 << i) == 0 {
                continue;
            }

            // Check counter suitability based on skip_match flag
            if !skip_match
                && (!is_counter_started(pmu_state, counter_idx)
                    || !can_monitor_event(
                        counter_idx,
                        pmu_state.hw_counters_num,
                        is_firmware_event,
                    ))
            {
                continue;
            }

            // Configure the counter (applies all flags)
            if configure_counter(pmu_state, counter_idx, event, flags, event_data) {
                return SbiRet::success(counter_idx);
            }
            return SbiRet::failed();
        }

        SbiRet::not_supported()
    }

    /// Start one or more counters (FID #3)
    /// Note: The next two functions contain redundant logic and should be refactored.
    #[inline]
    fn counter_start(
        &self,
        counter_idx_base: usize,
        counter_idx_mask: usize,
        start_flags: usize,
        initial_value: u64,
    ) -> SbiRet {
        let flags = match flags::CounterStartFlags::from_bits(start_flags) {
            Some(flags) => flags,
            None => return SbiRet::invalid_param(),
        };

        let pmu_state = &mut hart_context_mut(current_hartid()).pmu_state;

        if counter_idx_base >= pmu_state.total_counters_num
            || (counter_idx_mask & ((1 << pmu_state.total_counters_num) - 1)) == 0
        {
            return SbiRet::invalid_param();
        }

        if flags.contains(flags::CounterStartFlags::INIT_SNAPSHOT) {
            return SbiRet::no_shmem();
        }

        let effective_mask = counter_idx_mask & ((1 << pmu_state.total_counters_num) - 1);
        let max_counters = pmu_state
            .total_counters_num
            .saturating_sub(counter_idx_base);

        for i in 0..max_counters {
            let counter_idx = counter_idx_base + i;
            if effective_mask & (1 << i) == 0 {
                continue;
            }

            if is_counter_started(pmu_state, counter_idx) {
                return SbiRet::already_started();
            }

            if counter_idx >= pmu_state.hw_counters_num {
                let fw_idx = counter_idx - pmu_state.hw_counters_num;
                if fw_idx >= FIRMWARE_COUNTER_MAX {
                    return SbiRet::invalid_param();
                }
                if flags.contains(flags::CounterStartFlags::INIT_VALUE) {
                    pmu_state.fw_counter[fw_idx] = initial_value;
                }
                pmu_state.fw_counter_state |= 1 << fw_idx;
            } else {
                if flags.contains(flags::CounterStartFlags::INIT_VALUE) {
                    // TODO: Write initial value to CSR
                    // write_mhpmcounterN (counter_idx, initial_value)
                }
                // TODO: Enable counter via CSR
            }
        }

        SbiRet::success(0)
    }

    /// Stop one or more counters (FID #4)
    #[inline]
    fn counter_stop(
        &self,
        counter_idx_base: usize,
        counter_idx_mask: usize,
        stop_flags: usize,
    ) -> SbiRet {
        let flags = match flags::CounterStopFlags::from_bits(stop_flags) {
            Some(flags) => flags,
            None => return SbiRet::invalid_param(),
        };

        let pmu_state = &mut hart_context_mut(current_hartid()).pmu_state;

        if counter_idx_base >= pmu_state.total_counters_num
            || (counter_idx_mask & ((1 << pmu_state.total_counters_num) - 1)) == 0
        {
            return SbiRet::invalid_param();
        }

        if flags.contains(flags::CounterStopFlags::TAKE_SNAPSHOT) {
            return SbiRet::no_shmem();
        }

        let effective_mask = counter_idx_mask & ((1 << pmu_state.total_counters_num) - 1);
        let max_counters = pmu_state
            .total_counters_num
            .saturating_sub(counter_idx_base);

        for i in 0..max_counters {
            let counter_idx = counter_idx_base + i;
            if effective_mask & (1 << i) == 0 {
                continue;
            }

            if !is_counter_started(pmu_state, counter_idx) {
                return SbiRet::already_stopped();
            }

            if counter_idx >= pmu_state.hw_counters_num {
                let fw_idx = counter_idx - pmu_state.hw_counters_num;
                if fw_idx >= FIRMWARE_COUNTER_MAX {
                    return SbiRet::invalid_param();
                }
                pmu_state.fw_counter_state &= !(1 << fw_idx);
                if flags.contains(flags::CounterStopFlags::RESET) {
                    pmu_state.active_event[counter_idx] = 0;
                }
            } else {
                if flags.contains(flags::CounterStopFlags::RESET) {
                    pmu_state.active_event[counter_idx] = 0;
                }
                // TODO: Disable counter via CSR
            }
        }

        SbiRet::success(0)
    }

    /// Reads a firmware counter value
    #[inline]
    fn counter_fw_read(&self, counter_idx: usize) -> SbiRet {
        let pmu_state = &hart_context(current_hartid()).pmu_state;
        match pmu_state.get_event_idx(counter_idx, true) {
            Some(event_id) if event_id.is_firmware_event_valid() => {
                if event_id.event_code() == firmware_event::PLATFORM {
                    // TODO: Handle platform-specific PMU events
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

    #[inline]
    fn counter_fw_read_hi(&self, _counter_idx: usize) -> SbiRet {
        // The Specification states the this function  always returns zero in sbiret.value for RV64 (or higher) systems.
        // Currently RustSBI Prototyper only supports RV64 systems
        SbiRet::success(0)
    }

    #[inline]
    fn snapshot_set_shmem(&self, shmem: SharedPtr<[u8; SIZE]>, flags: usize) -> SbiRet {
        // Optional function, `not_supported` is returned if not implemented.
        let _ = (shmem, flags);
        SbiRet::not_supported()
    }
}

struct EventToCounterMap {
    counters_mask: u32,   // Bitmask of supported counters
    event_start_idx: u32, // Start of event code range
    event_end_id: u32,    // End of event code range
}

struct RawEventToCounterMap {
    counters_mask: u32,    // Bitmask of supported counters
    raw_event_select: u64, // Value to program into mhpmeventX
    select_mask: u64,      // Mask for selecting bits (optional use)
}

/// Configures a counter to monitor an event based on the given flags.
///
/// Returns `true` if configuration succeeds, `false` otherwise.
#[inline]
fn configure_counter(
    pmu_state: &mut PmuState,
    counter_idx: usize,
    event: EventIdx,
    flags: flags::CounterCfgFlags,
    event_data: u64,
) -> bool {
    todo!()
}

/// Checks if a counter is currently started.
///
/// Returns `true` if the counter is active (not inhibited), `false` otherwise.
#[inline]
fn is_counter_started(pmu_state: &PmuState, counter_idx: usize) -> bool {
    if counter_idx < pmu_state.hw_counters_num {
        // Hardware counter: Check mcountinhibit CSR
        let inhibit = riscv::register::mcountinhibit::read();
        (inhibit.bits() & (1 << counter_idx)) == 0
    } else {
        // Firmware counter: Check fw_counter_state bitmask
        let fw_idx = counter_idx - pmu_state.hw_counters_num;
        fw_idx < FIRMWARE_COUNTER_MAX && (pmu_state.fw_counter_state & (1 << fw_idx)) != 0
    }
}

#[inline]
fn can_monitor_event(counter_idx: usize, hw_counters_num: usize, is_firmware_event: bool) -> bool {
    if is_firmware_event {
        counter_idx >= hw_counters_num && counter_idx < (hw_counters_num + FIRMWARE_COUNTER_MAX)
    } else {
        counter_idx < hw_counters_num
    }
}

struct CounterInfo {
    inner: usize,
}

impl CounterInfo {
    const CSR_MASK: usize = 0xFFF; // Bits [11:0]
    const WIDTH_MASK: usize = 0x3F << 12; // Bits [17:12]
    const FIRMWARE_FLAG: usize = 1 << (size_of::<usize>() * 8 - 1); // MSB

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
struct EventIdx {
    inner: usize,
}

impl EventIdx {
    #[inline]
    pub const fn new(event_idx: usize) -> Self {
        Self { inner: event_idx }
    }

    #[inline]
    pub const fn event_type(self) -> usize {
        (self.inner >> 16) & 0xF
    }

    #[inline]
    pub const fn event_code(self) -> usize {
        self.inner & 0xFFFF
    }

    /// Extracts the cache ID for HARDWARE_CACHE events (13 bits, [15:3])
    #[inline]
    pub const fn cache_id(self) -> usize {
        (self.inner >> 3) & 0x1FFF
    }

    /// Extracts the cache operation ID (2 bits, [2:1])
    #[inline]
    pub const fn cache_op_id(self) -> usize {
        (self.inner >> 1) & 0x3
    }

    /// Extracts the cache result ID (1 bit, [0])
    #[inline]
    pub const fn cache_result_id(self) -> usize {
        self.inner & 0x1
    }

    #[inline]
    pub fn check_event_type(self) -> bool {
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
    pub fn is_firmware_event_valid(self) -> bool {
        self.event_code() <= firmware_event::HFENCE_VVMA_ASID_RECEIVED
            || (self.event_code() >= firmware_event::PLATFORM
                && self.event_code() != firmware_event::PLATFORM)
    }
}
