use core::sync::atomic::{AtomicBool, Ordering};

use rustsbi::SbiRet;
use sbi_spec::{
    binary::SharedPtr,
    sse::{attr_id, event_id},
};
use spin::Mutex;

/// Tracks local Supervisor Software Events (SSE) state for the prototyper.
///
/// The boot path keeps this extension unavailable because the SBI 3.0 context
/// switches for event delivery and completion are not implemented. Global
/// events are also not implemented.
pub(crate) struct SbiSse;

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

fn valid_event_id(event_id: u32) -> bool {
    event_id & 0x0000_4000 != 0
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

fn event_index(event_id: u32) -> Result<usize, SbiRet> {
    match SUPPORTED_EVENTS.iter().position(|&id| id == event_id) {
        Some(index) => Ok(index),
        None if valid_event_id(event_id) => Err(SbiRet::not_supported()),
        None => Err(SbiRet::invalid_param()),
    }
}

fn current_hart() -> usize {
    crate::riscv::current_hartid()
}

fn checked_supervisor_buffer(
    ptr: SharedPtr<u8>,
    base_attr_id: u32,
    attr_count: u32,
) -> Result<usize, SbiRet> {
    let start = ptr.phys_addr_lo();
    // Attribute buffers are XLEN-aligned. The prototyper currently accepts
    // only addresses whose upper physical-address word is zero.
    if start & (core::mem::size_of::<usize>() - 1) != 0 || ptr.phys_addr_hi() != 0 {
        return Err(SbiRet::invalid_address());
    }

    let attr_size = core::mem::size_of::<usize>();
    let Some(offset) = (base_attr_id as usize).checked_mul(attr_size) else {
        return Err(SbiRet::bad_range());
    };
    let Some(len) = (attr_count as usize).checked_mul(attr_size) else {
        return Err(SbiRet::invalid_param());
    };
    let Some(start) = start.checked_add(offset) else {
        return Err(SbiRet::invalid_address());
    };

    if !crate::firmware::supervisor_writable(start, len) {
        return Err(SbiRet::invalid_address());
    }

    Ok(start)
}

impl rustsbi::Sse for SbiSse {
    fn read_attrs(
        &self,
        event_id: u32,
        base_attr_id: u32,
        attr_count: u32,
        output: SharedPtr<u8>,
    ) -> SbiRet {
        let idx = match event_index(event_id) {
            Ok(idx) => idx,
            Err(err) => return err,
        };
        if attr_count == 0 {
            return SbiRet::invalid_param();
        }
        let base = match checked_supervisor_buffer(output, base_attr_id, attr_count) {
            Ok(base) => base as *mut u8,
            Err(err) => return err,
        };

        let events = EVENTS[current_hart()].lock();
        let event = &events[idx];
        for i in 0..attr_count {
            let Some(attr_id) = base_attr_id.checked_add(i) else {
                return SbiRet::bad_range();
            };
            let value = match attr_id {
                attr_id::STATUS => {
                    (event.state as usize) | ((event.pending as usize) << 2) | (1 << 3)
                }
                attr_id::PRIORITY => event.priority as usize,
                _ => return SbiRet::bad_range(),
            };
            // SAFETY: `checked_supervisor_buffer` validated the aligned output
            // span, and `i < attr_count` keeps this write within that span.
            unsafe {
                (base.add(i as usize * core::mem::size_of::<usize>()) as *mut usize)
                    .write_volatile(value.to_le());
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
        let idx = match event_index(event_id) {
            Ok(idx) => idx,
            Err(err) => return err,
        };
        if attr_count == 0 {
            return SbiRet::invalid_param();
        }
        let base = match checked_supervisor_buffer(input, base_attr_id, attr_count) {
            Ok(base) => base as *const u8,
            Err(err) => return err,
        };

        let mut events = EVENTS[current_hart()].lock();
        for i in 0..attr_count {
            let Some(attr_id) = base_attr_id.checked_add(i) else {
                return SbiRet::bad_range();
            };
            // SAFETY: `checked_supervisor_buffer` validated the aligned input
            // span, and `i < attr_count` keeps this read within that span.
            let value = unsafe {
                (base.add(i as usize * core::mem::size_of::<usize>()) as *const usize)
                    .read_volatile()
            };
            match attr_id {
                attr_id::STATUS => return SbiRet::denied(),
                attr_id::PRIORITY => {
                    if !matches!(
                        events[idx].state,
                        EventState::Unused | EventState::Registered
                    ) {
                        return SbiRet::invalid_state();
                    }
                    match u32::try_from(usize::from_le(value)) {
                        Ok(priority) => events[idx].priority = priority,
                        Err(_) => return SbiRet::invalid_param(),
                    }
                }
                _ => return SbiRet::bad_range(),
            }
        }
        SbiRet::success(0)
    }

    fn register(&self, event_id: u32, handler_entry_pc: usize, handler_entry_arg: usize) -> SbiRet {
        let idx = match event_index(event_id) {
            Ok(idx) => idx,
            Err(err) => return err,
        };
        if handler_entry_pc & 1 != 0 {
            return SbiRet::invalid_param();
        }
        let mut events = EVENTS[current_hart()].lock();
        let event = &mut events[idx];
        if event.state != EventState::Unused {
            return SbiRet::invalid_state();
        }
        event.handler_pc = handler_entry_pc;
        event.handler_arg = handler_entry_arg;
        event.state = EventState::Registered;
        SbiRet::success(0)
    }

    fn unregister(&self, event_id: u32) -> SbiRet {
        let idx = match event_index(event_id) {
            Ok(idx) => idx,
            Err(err) => return err,
        };
        let mut events = EVENTS[current_hart()].lock();
        let event = &mut events[idx];
        if event.state != EventState::Registered {
            return SbiRet::invalid_state();
        }
        event.handler_pc = 0;
        event.handler_arg = 0;
        event.state = EventState::Unused;
        SbiRet::success(0)
    }

    fn enable(&self, event_id: u32) -> SbiRet {
        let idx = match event_index(event_id) {
            Ok(idx) => idx,
            Err(err) => return err,
        };
        let mut events = EVENTS[current_hart()].lock();
        let event = &mut events[idx];
        if event.state != EventState::Registered {
            return SbiRet::invalid_state();
        }
        event.state = EventState::Enabled;
        let hart_id = current_hart();
        if event.pending && !HART_MASKED[hart_id].load(Ordering::Acquire) {
            // FIXME: Deliver the event before entering `Running`.
            event.state = EventState::Running;
            event.pending = false;
        }
        SbiRet::success(0)
    }

    fn disable(&self, event_id: u32) -> SbiRet {
        let idx = match event_index(event_id) {
            Ok(idx) => idx,
            Err(err) => return err,
        };
        let mut events = EVENTS[current_hart()].lock();
        let event = &mut events[idx];
        if event.state != EventState::Enabled {
            return SbiRet::invalid_state();
        }
        event.state = EventState::Registered;
        SbiRet::success(0)
    }

    fn complete(&self) -> SbiRet {
        // FIXME: Restore the interrupted context before leaving `Running`, and
        // unregister one-shot events.
        let mut events = EVENTS[current_hart()].lock();
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
        let idx = match event_index(event_id) {
            Ok(idx) => idx,
            Err(err) => return err,
        };
        let hart_enabled = crate::platform::cpu_enabled()
            .and_then(|enabled| enabled.get(hart_id).copied())
            .unwrap_or(false);
        if !hart_enabled {
            return SbiRet::invalid_param();
        }
        let Some(events) = EVENTS.get(hart_id) else {
            return SbiRet::invalid_param();
        };
        let mut events = events.lock();
        let event = &mut events[idx];
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
        let hart_id = current_hart();
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
        let hart_id = current_hart();
        if HART_MASKED[hart_id].swap(true, Ordering::AcqRel) {
            return SbiRet::already_stopped();
        }
        SbiRet::success(0)
    }
}
