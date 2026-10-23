//! Validated SBI events and platform hardware-event descriptions.

use super::FirmwareEvent;
use alloc::{collections::BTreeMap, vec::Vec};
use core::ops::RangeInclusive;
use runtime::pmu::HardwareCounter;
use runtime::{Error, FdtNode, Result};
use sbi_spec::pmu::{cache_event, cache_operation, event_type, firmware_event, hardware_event};

/// An SBI event identifier whose type and code have been validated.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct Event(u32);

impl TryFrom<usize> for Event {
    type Error = Error;

    fn try_from(raw: usize) -> Result<Self> {
        if raw >> 20 != 0 {
            return Err(Error::InvalidArgs);
        }
        let code = raw & 0xffff;
        let valid = match raw >> 16 {
            event_type::HARDWARE_GENERAL => (1..=hardware_event::REF_CPU_CYCLES).contains(&code),
            event_type::HARDWARE_CACHE => {
                code >> 3 <= cache_event::NODE && (code >> 1) & 3 <= cache_operation::PREFETCH
            }
            event_type::HARDWARE_RAW | event_type::HARDWARE_RAW_V2 => code == 0,
            event_type::FIRMWARE => code <= firmware_event::HFENCE_VVMA_ASID_RECEIVED,
            _ => false,
        };
        valid.then_some(Self(raw as u32)).ok_or(Error::InvalidArgs)
    }
}

impl From<FirmwareEvent> for Event {
    fn from(event: FirmwareEvent) -> Self {
        Self((event_type::FIRMWARE as u32) << 16 | event as u32)
    }
}

impl Event {
    pub(super) fn is_firmware(self) -> bool {
        self.0 >> 16 == event_type::FIRMWARE as u32
    }

    fn is_raw(self) -> bool {
        matches!(
            self.0 as usize >> 16,
            event_type::HARDWARE_RAW | event_type::HARDWARE_RAW_V2
        )
    }

    pub(super) fn fixed_counter(self) -> Option<HardwareCounter> {
        match self.0 as usize {
            hardware_event::CPU_CYCLES => Some(HardwareCounter::Cycle),
            hardware_event::INSTRUCTIONS => Some(HardwareCounter::InstructionsRetired),
            _ => None,
        }
    }
}

/// Event encoding and counter eligibility are separate platform facts.
#[derive(Default)]
pub(super) struct HardwareEvents {
    encoding: EventEncoding,
    counters: Vec<EventCounters>,
    raw_counters: Vec<RawEventCounters>,
}

#[derive(Default)]
enum EventEncoding {
    /// Platforms such as QEMU use the SBI event identifier directly.
    #[default]
    Identity,
    Table(BTreeMap<u32, u64>),
}

struct EventCounters {
    events: RangeInclusive<u32>,
    counters: u32,
}

struct RawEventCounters {
    value: u64,
    mask: u64,
    counters: u32,
}

impl HardwareEvents {
    /// Reads OpenSBI's PMU device-tree binding:
    /// <https://github.com/riscv-software-src/opensbi/blob/master/docs/pmu_support.md#sbi-pmu-device-tree-bindings>.
    /// Duplicate encodings and overlapping event ranges are rejected as local
    /// policy so event lookup does not depend on property order.
    pub(super) fn from_node(node: FdtNode<'_, '_>) -> Result<Self> {
        let mut events = Self::default();
        if let Some(property) = node.property("riscv,event-to-mhpmevent") {
            let mut table = BTreeMap::new();
            for [event, high, low] in rows(property.value)? {
                if table
                    .insert(event, (u64::from(high) << 32) | u64::from(low))
                    .is_some()
                {
                    return Err(Error::InvalidArgs);
                }
            }
            events.encoding = EventEncoding::Table(table);
        }
        if let Some(property) = node.property("riscv,event-to-mhpmcounters") {
            for [start, end, counters] in rows(property.value)? {
                if start > end
                    || events
                        .counters
                        .iter()
                        .any(|entry| start <= *entry.events.end() && *entry.events.start() <= end)
                {
                    return Err(Error::InvalidArgs);
                }
                events.counters.push(EventCounters {
                    events: start..=end,
                    counters,
                });
            }
        }
        if let Some(property) = node.property("riscv,raw-event-to-mhpmcounters") {
            for [high, low, mask_high, mask_low, counters] in rows(property.value)? {
                let value = (u64::from(high) << 32) | u64::from(low);
                let mask = (u64::from(mask_high) << 32) | u64::from(mask_low);
                if events
                    .raw_counters
                    .iter()
                    .any(|entry| entry.value == value && entry.mask == mask)
                {
                    return Err(Error::InvalidArgs);
                }
                events.raw_counters.push(RawEventCounters {
                    value,
                    mask,
                    counters,
                });
            }
        }
        Ok(events)
    }

    /// Returns architectural counter positions eligible for this event.
    pub(super) fn counters(&self, event: Event, data: u64) -> u32 {
        if event.is_raw() {
            self.raw_counters
                .iter()
                .find(|entry| data & entry.mask == entry.value)
                .map_or(0, |entry| entry.counters)
        } else {
            self.counters
                .iter()
                .find(|entry| entry.events.contains(&event.0))
                .map_or(0, |entry| entry.counters)
        }
    }

    /// Encodes an event selected for a programmable hardware counter.
    pub(super) fn encode(&self, event: Event, data: u64) -> Result<u64> {
        if event.is_raw() {
            return Ok(data);
        }
        match &self.encoding {
            EventEncoding::Identity => Ok(u64::from(event.0)),
            EventEncoding::Table(table) => table.get(&event.0).copied().ok_or(Error::NotSupported),
        }
    }
}

fn rows<const COLUMNS: usize>(bytes: &[u8]) -> Result<impl Iterator<Item = [u32; COLUMNS]> + '_> {
    let rows = bytes.chunks_exact(COLUMNS * size_of::<u32>());
    if !rows.remainder().is_empty() {
        return Err(Error::InvalidArgs);
    }
    Ok(rows.map(|row| {
        core::array::from_fn(|column| {
            let start = column * size_of::<u32>();
            u32::from_be_bytes(row[start..start + 4].try_into().unwrap())
        })
    }))
}
