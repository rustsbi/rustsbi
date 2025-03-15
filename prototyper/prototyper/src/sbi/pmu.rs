use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use riscv::register::*;
use rustsbi::{Pmu, SbiRet};
use sbi_spec::binary::SharedPtr;
use sbi_spec::pmu::shmem_size::SIZE;
use sbi_spec::pmu::*;

use crate::riscv::csr::*;
use crate::{riscv::current_hartid, sbi::features::hart_mhpm_mask};

use super::features::{PrivilegedVersion, hart_privileged_version};
use super::trap_stack::{hart_context, hart_context_mut};

/// Maximum number of hardware performance counters supported.
const HARDWARE_COUNTER_MAX: usize = 32;
/// Maximum number of firmware-managed counters supported.
const FIRMWARE_COUNTER_MAX: usize = 16;
/// Marker value for inactive/invalid event indices.
const PMU_EVENT_IDX_INVALID: usize = usize::MAX;
/// Bit mask for fixed counters (mcycle, time, minstret).
const PMU_FIXED_COUNTER_MASK: usize = 0x7;

/// PMU state tracking hardware and firmware performance counters
#[repr(C)]
pub struct PmuState {
    active_event: [usize; HARDWARE_COUNTER_MAX + FIRMWARE_COUNTER_MAX],
    /// Bitmap of active firmware counters (1 bit per counter)
    fw_counter_state: usize,
    /// Values for firmware-managed counters
    fw_counter: [u64; FIRMWARE_COUNTER_MAX],
    hw_counters_num: usize,
    total_counters_num: usize,
}

impl PmuState {
    /// Creates a new PMU state with default configuration.
    pub fn new() -> Self {
        let mhpm_mask = hart_mhpm_mask(current_hartid());
        let hw_counters_num = mhpm_mask.count_ones() as usize;
        let total_counters_num = hw_counters_num + FIRMWARE_COUNTER_MAX;

        let mut active_event = [PMU_EVENT_IDX_INVALID; HARDWARE_COUNTER_MAX + FIRMWARE_COUNTER_MAX];
        // Standard mappings for fixed counters
        active_event[0] = 0x1; // mcycle -> HW_CPU_CYCLES
        active_event[1] = 0x0; // time (memory-mapped)
        active_event[2] = 0x2; // minstret -> HW_INSTRUCTIONS

        Self {
            active_event,
            fw_counter_state: 0,
            fw_counter: [0; FIRMWARE_COUNTER_MAX],
            hw_counters_num,
            total_counters_num,
        }
    }

    /// Returns the number of hardware counters available.
    #[inline]
    pub fn get_hw_counter_num(&self) -> usize {
        self.hw_counters_num
    }

    /// Returns the total number of counters (hardware + firmware).
    #[inline]
    pub fn get_total_counters_num(&self) -> usize {
        self.total_counters_num
    }

    /// Gets the event index associated with a counter.
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

    /// Gets the value of a firmware counter.
    #[inline]
    pub fn get_fw_counter(&self, counter_idx: usize) -> Option<u64> {
        if counter_idx < self.hw_counters_num || counter_idx >= self.total_counters_num {
            return None;
        }
        let fw_idx = counter_idx - self.hw_counters_num;
        // Safety: fw_idx is guaranteed to be within bounds (0..FIRMWARE_COUNTER_MAX)
        unsafe { Some(*self.fw_counter.get_unchecked(fw_idx)) }
    }

    /// Updates a firmware counter with a new value.
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

struct SbiPmu {
    event_to_mhpmevent: Option<BTreeMap<u32, u64>>,
    event_to_mhpmcounter: Option<Vec<EventToCounterMap>>,
    raw_event_to_mhpmcounter: Option<Vec<RawEventToCounterMap>>,
}

impl Pmu for SbiPmu {
    /// Returns the total number of available performance counters
    ///
    /// Implements SBI PMU extension function (FID #0)
    #[inline]
    fn num_counters(&self) -> usize {
        hart_context(current_hartid()).pmu_state.total_counters_num
    }

    /// DONE:
    /// Function: Get details of a counter (FID #1)
    #[inline]
    fn counter_get_info(&self, counter_idx: usize) -> SbiRet {
        if counter_idx >= self.num_counters() {
            return SbiRet::invalid_param();
        }

        let pmu_state = &hart_context(current_hartid()).pmu_state;
        if counter_idx < pmu_state.hw_counters_num {
            let mask = hart_mhpm_mask(current_hartid());

            // Find the corresponding hardware counter using bit manipulation
            // This is more efficient than iterating through all possible offsets
            let mut remaining_mask = mask;
            let mut count = 0;

            while remaining_mask != 0 {
                if count == counter_idx {
                    // Found the counter - get its CSR offset
                    let offset = remaining_mask.trailing_zeros() as u16;
                    return SbiRet::success(
                        CounterInfo::with_hardware_info(CSR_CYCLE + offset, 63).inner(),
                    );
                }
                remaining_mask &= remaining_mask - 1;
                count += 1;
            }
            return SbiRet::invalid_param();
        }

        SbiRet::success(CounterInfo::with_firmware_info().inner())
    }

    /// TODO:
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
        let is_firmware_event = event.is_firmware_event();

        if counter_idx_base >= pmu_state.total_counters_num
            || (counter_idx_mask & ((1 << pmu_state.total_counters_num) - 1)) == 0
            || !event.check_event_type()
            || (is_firmware_event && !event.firmware_event_valid())
        {
            return SbiRet::invalid_param();
        }

        let skip_match = flags.contains(flags::CounterCfgFlags::SKIP_MATCH);

        let counter_idx;

        if skip_match {
            // If SKIP_MATCH is set, use the first counter in the mask without searching
            if let Some(ctr_idx) = CounterMask::new(counter_idx_base, counter_idx_mask).next() {
                if pmu_state.active_event[ctr_idx] == PMU_EVENT_IDX_INVALID {
                    return SbiRet::invalid_param();
                }
                counter_idx = ctr_idx;
            } else {
                return SbiRet::invalid_param();
            }
        } else {
            let match_result: Result<usize, SbiRet>;
            if event.is_firmware_event() {
                match_result = self.find_firmware_counter(
                    counter_idx_base,
                    counter_idx_mask,
                    event_idx,
                    pmu_state,
                );
            } else {
                match_result = self.find_hardware_counter(
                    counter_idx_base,
                    counter_idx_mask,
                    event_idx,
                    event_data,
                    pmu_state,
                );
            }
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

        if configure_counter(pmu_state, counter_idx, event, flags) {
            return SbiRet::success(counter_idx);
        }

        return SbiRet::not_supported();
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

        for counter_idx in CounterMask::new(counter_idx_base, counter_idx_mask) {
            if counter_idx >= pmu_state.total_counters_num {
                return SbiRet::invalid_param();
            }

            if is_counter_started(pmu_state, counter_idx) {
                return SbiRet::already_started();
            }

            if counter_idx >= pmu_state.hw_counters_num {
                let fw_idx = counter_idx - pmu_state.hw_counters_num;
                if fw_idx >= FIRMWARE_COUNTER_MAX {
                    return SbiRet::invalid_param();
                }

                // Initialize counter value if requested
                if flags.contains(flags::CounterStartFlags::INIT_VALUE) {
                    pmu_state.fw_counter[fw_idx] = initial_value;
                }
                pmu_state.fw_counter_state |= 1 << fw_idx;
            } else {
                let is_update_value = flags.contains(flags::CounterStartFlags::INIT_VALUE);
                let mhpm_offset = get_mhpm_csr_offset(counter_idx).unwrap();
                match start_hardware_counter(mhpm_offset, initial_value, is_update_value) {
                    Ok(_) => {}
                    Err(StartCounterErr::OffsetInvalid) => return SbiRet::invalid_param(),
                    Err(StartCounterErr::AlreadyStart) => return SbiRet::already_started(),
                }
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

        for counter_idx in CounterMask::new(counter_idx_base, counter_idx_mask) {
            if counter_idx >= pmu_state.total_counters_num {
                return SbiRet::invalid_param();
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
                    pmu_state.active_event[counter_idx] = PMU_EVENT_IDX_INVALID;
                }
            } else {
                // If RESET flag is set, mark the counter as inactive
                if flags.contains(flags::CounterStopFlags::RESET) {
                    pmu_state.active_event[counter_idx] = PMU_EVENT_IDX_INVALID;
                }
                let mhpm_offset = get_mhpm_csr_offset(counter_idx).unwrap();
                match stop_hardware_counter(mhpm_offset) {
                    Ok(()) => {}
                    Err(StopCounterErr::OffsetInvalid) => return SbiRet::invalid_param(),
                    Err(StopCounterErr::AlreadyStop) => return SbiRet::already_stopped(),
                }
            }
        }
        SbiRet::success(0)
    }

    /// Reads a firmware counter value
    /// Function: Read a firmware counter (FID #5).
    #[inline]
    fn counter_fw_read(&self, counter_idx: usize) -> SbiRet {
        let pmu_state = &hart_context(current_hartid()).pmu_state;
        match pmu_state.get_event_idx(counter_idx, true) {
            Some(event_id) if event_id.firmware_event_valid() => {
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

    /// Function: Read a firmware counter high bits (FID #6).
    #[inline]
    fn counter_fw_read_hi(&self, _counter_idx: usize) -> SbiRet {
        // The Specification states the this function always return zero in sbiret.value for RV64 (or higher) systems.
        // Currently RustSBI Prototyper only supports RV64 systems
        SbiRet::success(0)
    }

    /// Function: Set PMU snapshot shared memory (FID #7).
    #[inline]
    fn snapshot_set_shmem(&self, shmem: SharedPtr<[u8; SIZE]>, flags: usize) -> SbiRet {
        // Optional function, `not_supported` is returned if not implemented.
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
        // TODO: support `PLATFORM` event
        let event = EventIdx::new(event_idx);
        if !event.firmware_event_valid() {
            return Err(SbiRet::not_supported());
        }
        for counter_idx in CounterMask::new(counter_idx_base, counter_idx_mask) {
            // If counter idx is not a firmware counter index, skip this index
            if counter_idx < pmu_state.hw_counters_num
                || counter_idx >= pmu_state.total_counters_num
            {
                continue;
            }
            // If the firmware counter at this index is already occupied, skip this index
            if pmu_state.active_event[counter_idx] != PMU_EVENT_IDX_INVALID {
                continue;
            }
            return Ok(counter_idx);
        }
        return Err(SbiRet::not_supported());
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
        let mut hw_counters_mask = 0;
        // Find the counters available for the event.
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
            // event is general event or cache event
            if let Some(ref sbi_hw_event_map_vec) = self.event_to_mhpmcounter {
                for sbi_hw_event_map in sbi_hw_event_map_vec {
                    if sbi_hw_event_map.have_event(event_idx as u32) {
                        hw_counters_mask = sbi_hw_event_map.get_counter_mask();
                        break;
                    }
                }
            } else {
                return Err(SbiRet::not_supported());
            }
        }
        // mcycle, time, minstret cannot be used for other events.
        let can_use_counter_mask = hw_counters_mask as usize & (!PMU_FIXED_COUNTER_MASK);

        // Find a counter that meets the conditions from a set of counters
        for counter_idx in CounterMask::new(counter_idx_base, counter_idx_mask) {
            if counter_idx >= pmu_state.hw_counters_num {
                continue;
            }

            // If the counter idx corresponding to the hardware counter index cannot be used by the event,
            // or has already been used, skip this counter idx
            let mhpm_offset = get_mhpm_csr_offset(counter_idx).unwrap();
            if (can_use_counter_mask >> mhpm_offset) & 0x1 == 0
                || pmu_state.active_event[counter_idx] != PMU_EVENT_IDX_INVALID
            {
                continue;
            }
            // If the counter idx corresponding to the hardware counter index has already started counting, skip the counter
            if hart_privileged_version(current_hartid()) >= PrivilegedVersion::Version1_11 {
                let inhibit = riscv::register::mcountinhibit::read();
                if (inhibit.bits() & (1 << mhpm_offset)) == 0 {
                    continue;
                }
            }

            // Found a counter that meets the conditions - write the event value to the corresponding mhpmevent
            self.pmu_update_hardware_mhpmevent(mhpm_offset, event_idx, event_data)?;
            return Ok(counter_idx);
        }
        Err(SbiRet::not_supported())
    }

    fn pmu_update_hardware_mhpmevent(
        &self,
        mhpm_offset: u16,
        event_idx: usize,
        event_data: u64,
    ) -> Result<(), SbiRet> {
        // Validate counter offset range (only mhpmcounter3-31 are configurable)
        if mhpm_offset < 3 || mhpm_offset > 31 {
            return Err(SbiRet::not_supported());
        }

        let event = EventIdx::new(event_idx);

        // Determine the value to write to mhpmevent CSR
        let mhpmevent_val = if event.is_raw_event() {
            // For raw events, use the provided event_data directly
            event_data
        } else if let Some(ref event_to_mhpmevent) = self.event_to_mhpmevent {
            // For standard events, look up the corresponding mhpmevent value
            *event_to_mhpmevent
                .get(&(event_idx as u32))
                .ok_or(SbiRet::not_supported())?
        } else if self.event_to_mhpmcounter.is_some() {
            // Handle QEMU compatibility case:
            // When only event_to_mhpmcounter is available (like in QEMU),
            // use the event index directly as the raw event value
            event_idx as u64
        } else {
            // No mapping available for this event
            return Err(SbiRet::not_supported());
        };

        write_mhpmevent(mhpm_offset, mhpmevent_val);
        Ok(())
    }
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
) -> bool {
    let auto_start = flags.contains(flags::CounterCfgFlags::AUTO_START);
    let clear_value = flags.contains(flags::CounterCfgFlags::CLEAR_VALUE);
    if event.is_firmware_event() {
        let firmware_event_idx = counter_idx - pmu_state.hw_counters_num;
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
        if auto_start {
            return start_hardware_counter(mhpm_offset, 0, false).is_ok();
        }
    }
    true
}

/// Get the offset of the mhpmcounter CSR corresponding to counter_idx relative to mcycle
fn get_mhpm_csr_offset(counter_idx: usize) -> Option<u16> {
    let mhpm_mask = hart_mhpm_mask(current_hartid());
    let mut count = 0;
    for offset in 0..32 {
        if (mhpm_mask >> offset) & 1 == 1 {
            if count == counter_idx {
                return Some(offset as u16);
            }
            count += 1;
        }
    }
    None
}

/// Checks if a counter is currently started.
///
/// Returns `true` if the counter is active (not inhibited), `false` otherwise.
#[inline]
fn is_counter_started(pmu_state: &PmuState, counter_idx: usize) -> bool {
    if counter_idx < pmu_state.hw_counters_num {
        // Hardware counter: Check mcountinhibit CSR
        if hart_privileged_version(current_hartid()) >= PrivilegedVersion::Version1_11 {
            let inhibit = riscv::register::mcountinhibit::read();
            let mhpm_offset = get_mhpm_csr_offset(counter_idx).unwrap();
            return (inhibit.bits() & (1 << mhpm_offset)) == 0;
        } else {
            return pmu_state.active_event[counter_idx] != PMU_EVENT_IDX_INVALID;
        }
    } else {
        // Firmware counter: Check fw_counter_state bitmask
        let fw_idx = counter_idx - pmu_state.hw_counters_num;
        fw_idx < FIRMWARE_COUNTER_MAX && (pmu_state.fw_counter_state & (1 << fw_idx)) != 0
    }
}

/// Start Hardware Counter
enum StartCounterErr {
    OffsetInvalid,
    AlreadyStart,
}

/// Starts a hardware performance counter specified by the offset.
fn start_hardware_counter(
    mhpm_offset: u16,
    new_value: u64,
    is_update_value: bool,
) -> Result<(), StartCounterErr> {
    if mhpm_offset == 1 || mhpm_offset > 31 {
        return Err(StartCounterErr::OffsetInvalid);
    }

    if hart_privileged_version(current_hartid()) < PrivilegedVersion::Version1_11 {
        if is_update_value {
            write_mhpmcounter(mhpm_offset, new_value);
        }
        return Ok(());
    }

    // Check if counter is already running by testing the inhibit bit
    // A zero bit in mcountinhibit means the counter is running
    if mcountinhibit::read().bits() & (1 << mhpm_offset) == 0 {
        return Err(StartCounterErr::AlreadyStart);
    }

    if is_update_value {
        write_mhpmcounter(mhpm_offset, new_value);
    }

    unsafe {
        match mhpm_offset {
            0 => mcountinhibit::clear_cy(),
            2 => mcountinhibit::clear_ir(),
            _ => mcountinhibit::clear_hpm(mhpm_offset as usize),
        }
    }
    Ok(())
}

/// Stop Hardware Counter
enum StopCounterErr {
    OffsetInvalid,
    AlreadyStop,
}

/// Stops a hardware performance counter specified by the offset.
fn stop_hardware_counter(mhpm_offset: u16) -> Result<(), StopCounterErr> {
    if mhpm_offset == 1 || mhpm_offset > 31 {
        return Err(StopCounterErr::OffsetInvalid);
    }
    if hart_privileged_version(current_hartid()) < PrivilegedVersion::Version1_11 {
        return Ok(());
    }

    if mcountinhibit::read().bits() & (1 << mhpm_offset) != 0 {
        return Err(StopCounterErr::AlreadyStop);
    }

    unsafe {
        match mhpm_offset {
            0 => mcountinhibit::set_cy(),
            2 => mcountinhibit::set_ir(),
            _ => mcountinhibit::set_hpm(mhpm_offset as usize),
        }
    }
    Ok(())
}

/// Write MHPMEVENT or MHPMCOUNTER
fn write_mhpmevent(mhpm_offset: u16, mhpmevent_val: u64) {
    let csr = CSR_MHPMEVENT3 + mhpm_offset - 3;

    // Special cases for cycle and instret
    if csr == CSR_MCYCLE {
        crate::riscv::csr::mcycle::write(mhpmevent_val);
        return;
    } else if csr == CSR_MINSTRET {
        crate::riscv::csr::minstret::write(mhpmevent_val);
        return;
    }

    // Handle MHPMEVENT3-31
    if csr >= CSR_MHPMEVENT3 && csr <= CSR_MHPMEVENT31 {
        // Convert CSR value to register index (3-31)
        let idx = csr - CSR_MHPMEVENT3 + 3;
        macro_rules! write_event {
            ($($i:literal),*) => {
                $(
                    if idx == $i {
                        pastey::paste!{ [<mhpmevent $i>]::write(mhpmevent_val as usize) };
                    }
                )*
            }
        }

        // Use seq_macro to generate all valid indices from 3 to 31
        seq_macro::seq!(N in 3..=31 {
            write_event!(N);
        });
    }
}

fn write_mhpmcounter(mhpm_offset: u16, mhpmevent_val: u64) {
    let counter_idx = mhpm_offset;

    // Only handle valid counter indices (3-31)
    if counter_idx >= 3 && counter_idx <= 31 {
        macro_rules! write_counter {
            ($($i:literal),*) => {
                $(
                    if counter_idx == $i {
                        pastey::paste!{ [<mhpmcounter $i>]::write(mhpmevent_val as usize) };
                    }
                )*
            }
        }

        // Call the macro with all valid indices
        seq_macro::seq!(N in 3..=31 {
            write_counter!(N);
        });
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
pub struct EventIdx {
    /// Packed representation of event information:
    /// - Bits [15:0]: Event code
    /// - Bits [19:16]: Event type
    inner: usize,
}

impl EventIdx {
    #[inline]
    pub const fn new(event_idx: usize) -> Self {
        Self { inner: event_idx }
    }

    #[inline]
    pub const fn event_type(&self) -> usize {
        (self.inner >> 16) & 0xF
    }

    #[inline]
    pub const fn event_code(&self) -> usize {
        self.inner & 0xFFFF
    }

    /// Extracts the cache ID for HARDWARE_CACHE events (13 bits, [15:3])
    #[inline]
    pub const fn cache_id(&self) -> usize {
        (self.inner >> 3) & 0x1FFF
    }

    /// Extracts the cache operation ID (2 bits, [2:1])
    #[inline]
    pub const fn cache_op_id(&self) -> usize {
        (self.inner >> 1) & 0x3
    }

    /// Extracts the cache result ID (1 bit, [0])
    #[inline]
    pub const fn cache_result_id(&self) -> usize {
        self.inner & 0x1
    }

    #[inline]
    pub const fn is_general_event(&self) -> bool {
        self.event_type() == event_type::HARDWARE_GENERAL
    }

    #[inline]
    pub const fn is_cache_event(&self) -> bool {
        self.event_type() == event_type::HARDWARE_CACHE
    }

    #[inline]
    pub const fn is_raw_event_v1(&self) -> bool {
        self.event_type() == event_type::HARDWARE_RAW
    }

    #[inline]
    pub const fn is_raw_event_v2(&self) -> bool {
        self.event_type() == event_type::HARDWARE_RAW_V2
    }

    #[inline]
    pub const fn is_raw_event(&self) -> bool {
        self.is_raw_event_v1() || self.is_raw_event_v2()
    }

    #[inline]
    pub const fn is_firmware_event(&self) -> bool {
        self.event_type() == event_type::FIRMWARE
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
    pub fn firmware_event_valid(self) -> bool {
        let event_type = self.event_type();
        let event_code = self.event_code();
        if event_type != event_type::FIRMWARE {
            return false;
        }
        if (event_code > firmware_event::HFENCE_VVMA_ASID_RECEIVED
            && event_code < firmware_event::PLATFORM)
            || event_code >= firmware_event::PLATFORM
        {
            // TODO:Currently RustSBI Prototyper does not support PLATFORM practice
            return false;
        }
        true
    }
}

/// event to mhpmcounter map
struct EventToCounterMap {
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
    pub fn is_overlop(&self, other_map: &EventToCounterMap) -> bool {
        if (self.event_end_idx < other_map.event_start_idx
            && self.event_end_idx < other_map.event_end_idx)
            || (self.event_start_idx > other_map.event_start_idx
                && self.event_start_idx > other_map.event_end_idx)
        {
            return false;
        }
        true
    }

    #[inline]
    pub fn can_use_counter(&self, counter_idx: usize) -> bool {
        let pmu_state = &hart_context_mut(current_hartid()).pmu_state;
        if counter_idx >= pmu_state.hw_counters_num {
            return false;
        }
        if let Some(mhpm_offset) = get_mhpm_csr_offset(counter_idx) {
            return self.counters_mask & (1 << mhpm_offset) != 0;
        } else {
            return false;
        }
    }
}

struct RawEventToCounterMap {
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
    pub const fn is_overlop(&self, other_map: &RawEventToCounterMap) -> bool {
        self.select_mask == other_map.select_mask
            && self.raw_event_select == other_map.raw_event_select
    }

    #[inline]
    pub fn can_use_counter(&self, counter_idx: usize) -> bool {
        let pmu_state = &hart_context(current_hartid()).pmu_state;
        if counter_idx >= pmu_state.hw_counters_num {
            return false;
        }
        if let Some(mhpm_offset) = get_mhpm_csr_offset(counter_idx) {
            return self.counters_mask & (1 << mhpm_offset) != 0;
        } else {
            return false;
        }
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
            return None;
        } else {
            let low_bit = self.counter_idx_mask.trailing_zeros();
            let hart_id = usize::try_from(low_bit).unwrap() + self.counter_idx_base;
            self.counter_idx_mask &= !(1usize << low_bit);
            Some(hart_id)
        }
    }
}
