//! Supervisor software events.
//!
//! # References
//!
//! - Specification: [RISC-V SBI SSE extension](https://docs.riscv.org/reference/sbi/v3.0/ext-sse.html) —
//!   event attributes, shared-memory layout, and state transitions.

#![forbid(unsafe_code)]

use bitflags::bitflags;
use core::mem::{align_of, size_of};
use core::sync::atomic::{AtomicBool, Ordering};

use runtime::memory::{PhysAddr, PhysAddrRange, SupervisorMemory};
use rustsbi::SbiRet;
use sbi_spec::{
    binary::SharedPtr,
    sse::{attr_id, event_id},
};
use spin::Mutex;

use crate::riscv::current_hartid;

/// Tracks local Supervisor Software Events (SSE) state for the prototyper.
///
/// The boot path keeps this extension unavailable because the SBI 3.0 context
/// switches for event delivery and completion are not implemented. Global
/// events are also not implemented.
pub(crate) struct SbiSse {
    supervisor_memory: &'static SupervisorMemory,
}

impl SbiSse {
    fn attribute_buffer_start(
        &self,
        ptr: SharedPtr<u8>,
        base_attr_id: u32,
        attr_count: u32,
    ) -> Result<PhysAddr, SbiRet> {
        let start = PhysAddr::new(ptr.phys_addr_lo());
        // Attribute buffers are XLEN-aligned. The prototyper currently accepts
        // only addresses whose upper physical-address word is zero.
        if !start.is_aligned_to(align_of::<usize>()) || ptr.phys_addr_hi() != 0 {
            return Err(SbiRet::invalid_address());
        }

        let attribute_size = size_of::<usize>();
        let Some(buffer_offset) = (base_attr_id as usize).checked_mul(attribute_size) else {
            return Err(SbiRet::bad_range());
        };
        let Some(buffer_size) = (attr_count as usize).checked_mul(attribute_size) else {
            return Err(SbiRet::invalid_param());
        };
        let Some(start) = start.checked_add(buffer_offset) else {
            return Err(SbiRet::invalid_address());
        };

        let range = PhysAddrRange::from_start_len(start, buffer_size)
            .map_err(|_| SbiRet::invalid_address())?;
        if self.supervisor_memory.check_range(range).is_err() {
            return Err(SbiRet::invalid_address());
        }

        Ok(start)
    }
}

// Event sources represented by the state tracker.
const SUPPORTED_EVENTS: &[u32] = &[event_id::SOFTWARE_INJECTED_LOCAL];

const EVENT_COUNT: usize = SUPPORTED_EVENTS.len();

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
enum EventState {
    Unused = 0,
    Registered = 1,
    Enabled = 2,
    Running = 3,
}

bitflags! {
    struct EventStatus: usize {
        const PENDING = 1 << 2;
        const INJECTION_ALLOWED = 1 << 3;
    }
}

#[derive(Clone, Copy)]
struct EventRecord {
    // FIXME: Model CONFIG through INTERRUPTED_A7 (attribute IDs 2 through 9)
    // before the prototyper advertises SSE support.
    state: EventState,
    pending: bool,
    handler_pc: usize,
    handler_arg: usize,
    priority: u32,
}

impl EventRecord {
    const UNUSED: Self = Self {
        state: EventState::Unused,
        pending: false,
        handler_pc: 0,
        handler_arg: 0,
        priority: 0,
    };
}

static EVENTS: [Mutex<[EventRecord; EVENT_COUNT]>; crate::cfg::NUM_HART_MAX] =
    [const { Mutex::new([EventRecord::UNUSED; EVENT_COUNT]) }; crate::cfg::NUM_HART_MAX];
static HART_MASKED: [AtomicBool; crate::cfg::NUM_HART_MAX] =
    [const { AtomicBool::new(true) }; crate::cfg::NUM_HART_MAX];

fn is_valid_event_id(event_id: u32) -> bool {
    // Bit 14 selects the custom event range in each 16-bit event-ID group.
    const CUSTOM_EVENT_RANGE: u32 = 1 << 14;
    event_id & CUSTOM_EVENT_RANGE != 0
        || matches!(
            event_id,
            event_id::LOCAL_HIGH_PRIORITY_RAS
                | event_id::LOCAL_DOUBLE_TRAP
                | event_id::GLOBAL_HIGH_PRIORITY_RAS
                | event_id::LOCAL_PMU_OVERFLOW
                | event_id::LOCAL_LOW_PRIORITY_RAS
                | event_id::GLOBAL_LOW_PRIORITY_RAS
                | event_id::SOFTWARE_INJECTED_LOCAL
                | event_id::SOFTWARE_INJECTED_GLOBAL
        )
}

fn supported_event_index(event_id: u32) -> Result<usize, SbiRet> {
    match SUPPORTED_EVENTS.iter().position(|&id| id == event_id) {
        Some(index) => Ok(index),
        None if is_valid_event_id(event_id) => Err(SbiRet::not_supported()),
        None => Err(SbiRet::invalid_param()),
    }
}

impl rustsbi::Sse for SbiSse {
    fn read_attrs(
        &self,
        event_id: u32,
        base_attr_id: u32,
        attr_count: u32,
        output: SharedPtr<u8>,
    ) -> SbiRet {
        let event_index = match supported_event_index(event_id) {
            Ok(event_index) => event_index,
            Err(error) => return error,
        };
        if attr_count == 0 {
            return SbiRet::invalid_param();
        }
        let buffer_start = match self.attribute_buffer_start(output, base_attr_id, attr_count) {
            Ok(buffer_start) => buffer_start,
            Err(error) => return error,
        };

        let event = EVENTS[current_hartid()].lock()[event_index];
        for attribute_offset in 0..attr_count {
            let Some(attribute_id) = base_attr_id.checked_add(attribute_offset) else {
                return SbiRet::bad_range();
            };
            let value = match attribute_id {
                attr_id::STATUS => {
                    let mut status = EventStatus::INJECTION_ALLOWED;
                    status.set(EventStatus::PENDING, event.pending);
                    (event.state as usize) | status.bits()
                }
                attr_id::PRIORITY => event.priority as usize,
                _ => return SbiRet::bad_range(),
            };
            let byte_offset = (attribute_offset as usize)
                .checked_mul(size_of::<usize>())
                .expect("BUG: validated SSE attribute buffer access failed");
            let address = buffer_start
                .checked_add(byte_offset)
                .expect("BUG: validated SSE attribute buffer access failed");
            if self
                .supervisor_memory
                .write(address, &value.to_le_bytes())
                .is_err()
            {
                return SbiRet::invalid_address();
            }
        }
        SbiRet::success(0)
    }

    fn write_attrs(
        &self,
        event_id: u32,
        base_attr_id: u32,
        attr_count: u32,
        input: SharedPtr<u8>,
    ) -> SbiRet {
        let event_index = match supported_event_index(event_id) {
            Ok(event_index) => event_index,
            Err(error) => return error,
        };
        if attr_count == 0 {
            return SbiRet::invalid_param();
        }
        let buffer_start = match self.attribute_buffer_start(input, base_attr_id, attr_count) {
            Ok(buffer_start) => buffer_start,
            Err(error) => return error,
        };

        for attribute_offset in 0..attr_count {
            let Some(attribute_id) = base_attr_id.checked_add(attribute_offset) else {
                return SbiRet::bad_range();
            };
            let byte_offset = (attribute_offset as usize)
                .checked_mul(size_of::<usize>())
                .expect("BUG: validated SSE attribute buffer access failed");
            let address = buffer_start
                .checked_add(byte_offset)
                .expect("BUG: validated SSE attribute buffer access failed");
            let mut bytes = [0; size_of::<usize>()];
            // The synchronous SSE ecall keeps its input buffer stable until
            // this operation returns.
            if self.supervisor_memory.read(address, &mut bytes).is_err() {
                return SbiRet::invalid_address();
            }
            let value = usize::from_le_bytes(bytes);
            match attribute_id {
                attr_id::STATUS => return SbiRet::denied(),
                attr_id::PRIORITY => {
                    let mut events = EVENTS[current_hartid()].lock();
                    if !matches!(
                        events[event_index].state,
                        EventState::Unused | EventState::Registered
                    ) {
                        return SbiRet::invalid_state();
                    }
                    match u32::try_from(value) {
                        Ok(priority) => events[event_index].priority = priority,
                        Err(_) => return SbiRet::invalid_param(),
                    }
                }
                _ => return SbiRet::bad_range(),
            }
        }
        SbiRet::success(0)
    }

    fn register(&self, event_id: u32, handler_entry_pc: usize, handler_entry_arg: usize) -> SbiRet {
        let event_index = match supported_event_index(event_id) {
            Ok(event_index) => event_index,
            Err(error) => return error,
        };
        const INSTRUCTION_ALIGNMENT: usize = align_of::<u16>();
        if !handler_entry_pc.is_multiple_of(INSTRUCTION_ALIGNMENT) {
            return SbiRet::invalid_param();
        }
        let mut events = EVENTS[current_hartid()].lock();
        let event = &mut events[event_index];
        if event.state != EventState::Unused {
            return SbiRet::invalid_state();
        }
        event.handler_pc = handler_entry_pc;
        event.handler_arg = handler_entry_arg;
        event.state = EventState::Registered;
        SbiRet::success(0)
    }

    fn unregister(&self, event_id: u32) -> SbiRet {
        let event_index = match supported_event_index(event_id) {
            Ok(event_index) => event_index,
            Err(error) => return error,
        };
        let mut events = EVENTS[current_hartid()].lock();
        let event = &mut events[event_index];
        if event.state != EventState::Registered {
            return SbiRet::invalid_state();
        }
        event.handler_pc = 0;
        event.handler_arg = 0;
        event.state = EventState::Unused;
        SbiRet::success(0)
    }

    fn enable(&self, event_id: u32) -> SbiRet {
        let event_index = match supported_event_index(event_id) {
            Ok(event_index) => event_index,
            Err(error) => return error,
        };
        let mut events = EVENTS[current_hartid()].lock();
        let event = &mut events[event_index];
        if event.state != EventState::Registered {
            return SbiRet::invalid_state();
        }
        event.state = EventState::Enabled;
        let hart_id = current_hartid();
        if event.pending && !HART_MASKED[hart_id].load(Ordering::Acquire) {
            // FIXME: Deliver the event before entering `Running`.
            event.state = EventState::Running;
            event.pending = false;
        }
        SbiRet::success(0)
    }

    fn disable(&self, event_id: u32) -> SbiRet {
        let event_index = match supported_event_index(event_id) {
            Ok(event_index) => event_index,
            Err(error) => return error,
        };
        let mut events = EVENTS[current_hartid()].lock();
        let event = &mut events[event_index];
        if event.state != EventState::Enabled {
            return SbiRet::invalid_state();
        }
        event.state = EventState::Registered;
        SbiRet::success(0)
    }

    fn complete(&self) -> SbiRet {
        // FIXME: Restore the interrupted context before leaving `Running`, and
        // unregister one-shot events.
        let mut events = EVENTS[current_hartid()].lock();
        if let Some(event) = events
            .iter_mut()
            .filter(|event| event.state == EventState::Running)
            .min_by_key(|event| event.priority)
        {
            event.state = EventState::Enabled;
        }
        SbiRet::success(0)
    }

    fn inject(&self, event_id: u32, hart_id: usize) -> SbiRet {
        let event_index = match supported_event_index(event_id) {
            Ok(event_index) => event_index,
            Err(error) => return error,
        };
        let hart_enabled = crate::platform::enabled_harts()
            .and_then(|enabled| enabled.get(hart_id).copied())
            .unwrap_or(false);
        if !hart_enabled {
            return SbiRet::invalid_param();
        }
        let Some(events) = EVENTS.get(hart_id) else {
            return SbiRet::invalid_param();
        };
        let mut events = events.lock();
        let event = &mut events[event_index];
        event.pending = true;
        if event.state == EventState::Enabled && !HART_MASKED[hart_id].load(Ordering::Acquire) {
            // FIXME: Save the target hart context and redirect it to ENTRY_PC
            // before entering `Running`.
            event.state = EventState::Running;
            event.pending = false;
        }
        SbiRet::success(0)
    }

    fn hart_unmask(&self) -> SbiRet {
        let hart_id = current_hartid();
        if !HART_MASKED[hart_id].swap(false, Ordering::AcqRel) {
            return SbiRet::already_started();
        }
        let mut events = EVENTS[hart_id].lock();
        if let Some(event) = events
            .iter_mut()
            .filter(|event| event.state == EventState::Enabled && event.pending)
            .min_by_key(|event| event.priority)
        {
            // FIXME: Deliver the event before entering `Running`.
            event.state = EventState::Running;
            event.pending = false;
        }
        SbiRet::success(0)
    }

    fn hart_mask(&self) -> SbiRet {
        let hart_id = current_hartid();
        if HART_MASKED[hart_id].swap(true, Ordering::AcqRel) {
            return SbiRet::already_stopped();
        }
        SbiRet::success(0)
    }
}
