//! Device-tree event mappings and matching against validated counter selections.

use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use runtime::FdtNode;
use runtime::pmu::{CounterError, CounterKind, HardwareCounter};
use sbi_spec::pmu::*;

use super::counter_mask::CounterMask;
use super::state::{PMU_EVENT_IDX_INVALID, PmuState};

/// Event-selection or hardware-matching failure.
pub(super) enum EventError {
    Unavailable,
    Counter(CounterError),
}

impl From<CounterError> for EventError {
    fn from(error: CounterError) -> Self {
        Self::Counter(error)
    }
}

#[derive(Default)]
pub(crate) struct SbiPmu {
    event_to_mhpmevent: Option<BTreeMap<u32, u64>>,
    event_to_mhpmcounter: Option<Vec<EventToCounterMap>>,
    raw_event_to_mhpmcounter: Option<Vec<RawEventToCounterMap>>,
}

impl SbiPmu {
    pub(super) fn find_firmware_counter(
        &self,
        counters: CounterMask,
        pmu_state: &PmuState,
    ) -> Result<usize, EventError> {
        for counter_idx in counters {
            if counter_idx < pmu_state.hardware_counter_count()
                || counter_idx >= pmu_state.counter_count()
            {
                continue;
            }

            if pmu_state.active_event[counter_idx] != PMU_EVENT_IDX_INVALID {
                continue;
            }
            return Ok(counter_idx);
        }
        Err(EventError::Unavailable)
    }

    pub(super) fn find_hardware_counter(
        &self,
        counters: CounterMask,
        event_idx: usize,
        event_data: u64,
        pmu_state: &PmuState,
    ) -> Result<usize, EventError> {
        let event = EventIdx::new(event_idx);
        // Fixed counters have architectural event assignments even when
        // the device tree provides no programmable-counter mapping.
        let fixed_counter = counters.into_iter().find(|&idx| {
            matches!(
                (
                    event_idx,
                    pmu_state.hardware_counter(idx).map(HardwareCounter::kind)
                ),
                (hardware_event::CPU_CYCLES, Ok(CounterKind::Cycle))
                    | (hardware_event::INSTRUCTIONS, Ok(CounterKind::Instructions))
            )
        });
        let mut hw_counters_mask = 0;

        if event.is_raw_event() {
            if let Some(ref raw_event_map_vec) = self.raw_event_to_mhpmcounter {
                for raw_event_map in raw_event_map_vec {
                    if raw_event_map.contains_event(event_data) {
                        hw_counters_mask = raw_event_map.counter_mask();
                        break;
                    }
                }
            } else {
                return Err(EventError::Unavailable);
            }
        } else {
            if let Some(ref sbi_hw_event_map_vec) = self.event_to_mhpmcounter {
                for sbi_hw_event_map in sbi_hw_event_map_vec {
                    if sbi_hw_event_map.contains_event(event_idx as u32) {
                        hw_counters_mask = sbi_hw_event_map.counter_mask();
                        break;
                    }
                }
            } else {
                return fixed_counter.ok_or(EventError::Unavailable);
            }
        }

        for counter_idx in counters {
            if counter_idx >= pmu_state.hw_counters_num {
                continue;
            }

            let counter = match pmu_state.hardware_counter(counter_idx) {
                Ok(counter) => counter,
                Err(CounterError::TimeCounter) => continue,
                Err(error) => return Err(error.into()),
            };
            if !counter.matches_mask(hw_counters_mask)
                || pmu_state.active_event[counter_idx] != PMU_EVENT_IDX_INVALID
            {
                continue;
            }
            if matches!(counter.kind(), CounterKind::Cycle)
                && event_idx != hardware_event::CPU_CYCLES
                || matches!(counter.kind(), CounterKind::Instructions)
                    && event_idx != hardware_event::INSTRUCTIONS
            {
                continue;
            }
            if counter.is_running()? == Some(true) {
                continue;
            }

            return Ok(counter_idx);
        }
        fixed_counter.ok_or(EventError::Unavailable)
    }

    /// Maps an SBI event without changing the selected hardware counter.
    pub(super) fn hardware_event_selector(
        &self,
        counter: HardwareCounter,
        event_idx: usize,
        event_data: u64,
    ) -> Result<Option<u64>, EventError> {
        if matches!(
            (counter.kind(), event_idx),
            (CounterKind::Cycle, hardware_event::CPU_CYCLES)
                | (CounterKind::Instructions, hardware_event::INSTRUCTIONS)
        ) {
            return Ok(None);
        }
        if counter.kind() != CounterKind::Programmable {
            return Err(EventError::Unavailable);
        }

        let event = EventIdx::new(event_idx);

        let mhpmevent_val = if event.is_raw_event() {
            event_data
        } else if let Some(ref event_to_mhpmevent) = self.event_to_mhpmevent {
            *event_to_mhpmevent
                .get(&(event_idx as u32))
                .ok_or(EventError::Unavailable)?
        } else if self.event_to_mhpmcounter.is_some() {
            // QEMU supplies counter mappings without a separate selector mapping;
            // in that layout, SBI event indices are also hardware selector values.
            event_idx as u64
        } else {
            return Err(EventError::Unavailable);
        };

        Ok(Some(mhpmevent_val))
    }

    fn insert_event_to_mhpmevent(&mut self, event: u32, mhpmevent: u64) {
        let event_to_mhpmevent_map = self.event_to_mhpmevent.get_or_insert_default();

        if let Some(mhpmevent_mapped) = event_to_mhpmevent_map.get(&event) {
            error!(
                "Try to map event:0x{:08x} to mhpmevent:0x{:016x}, but the event has been mapped to mhpmevent:{}, please check the device tree file",
                event, mhpmevent, mhpmevent_mapped
            );
        } else {
            event_to_mhpmevent_map.insert(event, mhpmevent);
        }
    }

    fn insert_event_to_mhpmcounter(&mut self, event_to_counter: EventToCounterMap) {
        let event_to_mhpmcounter_map = self.event_to_mhpmcounter.get_or_insert_default();
        for event_to_mhpmcounter in event_to_mhpmcounter_map.iter() {
            if event_to_mhpmcounter.overlaps(&event_to_counter) {
                error!(
                    "The mapping of event_to_mhpmcounter {:?} and {:?} overlap, please check the device tree file",
                    event_to_mhpmcounter, event_to_counter
                );
                return;
            }
        }
        event_to_mhpmcounter_map.push(event_to_counter);
    }

    fn insert_raw_event_to_mhpmcounter(&mut self, raw_event_to_counter: RawEventToCounterMap) {
        let raw_event_to_mhpmcounter_map = self.raw_event_to_mhpmcounter.get_or_insert_default();
        for raw_event_to_mhpmcounter in raw_event_to_mhpmcounter_map.iter() {
            if raw_event_to_mhpmcounter.overlaps(&raw_event_to_counter) {
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

#[derive(Clone, Copy)]
pub(super) struct EventIdx {
    /// Packed event information.
    ///
    /// - Bits `[15:0]`: Event code.
    /// - Bits `[19:16]`: Event type.
    inner: usize,
}

impl EventIdx {
    pub(super) const fn new(event_idx: usize) -> Self {
        Self { inner: event_idx }
    }

    pub(super) fn from_firmware_event(firmware_event: usize) -> Self {
        Self {
            inner: 0xf << 16 | firmware_event,
        }
    }

    pub(super) fn raw(&self) -> usize {
        self.inner
    }

    const fn event_type(&self) -> usize {
        (self.inner >> 16) & 0xF
    }

    pub(super) const fn event_code(&self) -> usize {
        self.inner & 0xFFFF
    }

    /// Extracts the cache ID for `HARDWARE_CACHE` events (13 bits, `[15:3]`).
    const fn cache_id(&self) -> usize {
        (self.inner >> 3) & 0x1FFF
    }

    /// Extracts the cache operation ID (2 bits, `[2:1]`).
    const fn cache_op_id(&self) -> usize {
        (self.inner >> 1) & 0x3
    }

    /// Extracts the cache result ID (1 bit, `[0]`).
    const fn cache_result_id(&self) -> usize {
        self.inner & 0x1
    }

    const fn is_raw_event_v1(&self) -> bool {
        self.event_type() == event_type::HARDWARE_RAW
    }

    const fn is_raw_event_v2(&self) -> bool {
        self.event_type() == event_type::HARDWARE_RAW_V2
    }

    const fn is_raw_event(&self) -> bool {
        self.is_raw_event_v1() || self.is_raw_event_v2()
    }

    pub(super) const fn is_firmware_event(&self) -> bool {
        self.event_type() == event_type::FIRMWARE
    }

    pub(super) fn check_event_type(self) -> bool {
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

    pub(super) fn firmware_event_valid(self) -> bool {
        // Platform-specific firmware events have no mapping in this implementation.
        self.event_type() == event_type::FIRMWARE
            && self.event_code() <= firmware_event::HFENCE_VVMA_ASID_RECEIVED
    }
}

/// An inclusive SBI event range and its architectural hardware counter positions.
#[derive(Debug)]
struct EventToCounterMap {
    counters_mask: u32,
    event_start_idx: u32,
    event_end_idx: u32,
}

impl EventToCounterMap {
    fn new(counters_mask: u32, event_start_idx: u32, event_end_idx: u32) -> Self {
        Self {
            counters_mask,
            event_start_idx,
            event_end_idx,
        }
    }

    const fn contains_event(&self, event_idx: u32) -> bool {
        event_idx >= self.event_start_idx && event_idx <= self.event_end_idx
    }

    fn counter_mask(&self) -> u32 {
        self.counters_mask
    }

    fn overlaps(&self, other_map: &EventToCounterMap) -> bool {
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
struct RawEventToCounterMap {
    counters_mask: u32,
    raw_event_select: u64,
    select_mask: u64,
}

impl RawEventToCounterMap {
    fn new(counters_mask: u32, raw_event_select: u64, select_mask: u64) -> Self {
        Self {
            counters_mask,
            raw_event_select,
            select_mask,
        }
    }

    const fn contains_event(&self, event_idx: u64) -> bool {
        self.raw_event_select == (event_idx & self.select_mask)
    }

    const fn counter_mask(&self) -> u32 {
        self.counters_mask
    }

    const fn overlaps(&self, other_map: &RawEventToCounterMap) -> bool {
        self.select_mask == other_map.select_mask
            && self.raw_event_select == other_map.raw_event_select
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
