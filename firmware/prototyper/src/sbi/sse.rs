use core::sync::atomic::{AtomicBool, Ordering};

use rustsbi::SbiRet;
use sbi_spec::binary::SharedPtr;
use spin::Mutex;

/// Implementation of SBI Supervisor Software Events (SSE) extension.
///
/// This is a minimal local-event implementation mirroring the OpenSBI SSE
/// state machine. Global events are rejected as not supported.
pub(crate) struct SbiSse;

/// Platform-supported local SSE event IDs.
const SUPPORTED_EVENTS: &[u32] = &[0x1, 0x2, 0x3];

const EVENT_COUNT: usize = SUPPORTED_EVENTS.len();

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
enum EventState {
    Unused = 0,
    Registered = 1,
    Enabled = 2,
    Disabled = 3,
    Running = 4,
}

#[derive(Clone, Copy)]
struct EventRecord {
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

fn event_index(event_id: u32) -> Option<usize> {
    SUPPORTED_EVENTS.iter().position(|&id| id == event_id)
}

fn current_hart() -> usize {
    crate::riscv::current_hartid()
}

fn checked_supervisor_buffer(ptr: SharedPtr<u8>, len: usize) -> Result<usize, SbiRet> {
    if ptr.phys_addr_hi() != 0 {
        return Err(SbiRet::invalid_param());
    }

    let start = ptr.phys_addr_lo();
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
        let Some(idx) = event_index(event_id) else {
            return SbiRet::not_supported();
        };
        if attr_count == 0 {
            return SbiRet::invalid_param();
        }
        let Some(len) = (attr_count as usize).checked_mul(4) else {
            return SbiRet::invalid_param();
        };
        let base = match checked_supervisor_buffer(output, len) {
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
                0 => event.state as u32,
                1 => event.priority,
                _ => return SbiRet::bad_range(),
            };
            unsafe {
                (base.add(i as usize * 4) as *mut u32).write_volatile(value.to_le());
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
        let Some(idx) = event_index(event_id) else {
            return SbiRet::not_supported();
        };
        if attr_count == 0 {
            return SbiRet::invalid_param();
        }
        let Some(len) = (attr_count as usize).checked_mul(4) else {
            return SbiRet::invalid_param();
        };
        let base = match checked_supervisor_buffer(input, len) {
            Ok(base) => base as *const u8,
            Err(err) => return err,
        };

        let mut events = EVENTS[current_hart()].lock();
        for i in 0..attr_count {
            let Some(attr_id) = base_attr_id.checked_add(i) else {
                return SbiRet::bad_range();
            };
            let value = unsafe { (base.add(i as usize * 4) as *const u32).read_volatile() };
            match attr_id {
                0 => return SbiRet::denied(),
                1 => events[idx].priority = value,
                _ => return SbiRet::bad_range(),
            }
        }
        SbiRet::success(0)
    }

    fn register(&self, event_id: u32, handler_entry_pc: usize, handler_entry_arg: usize) -> SbiRet {
        let Some(idx) = event_index(event_id) else {
            return SbiRet::not_supported();
        };
        let mut events = EVENTS[current_hart()].lock();
        let event = &mut events[idx];
        if event.state != EventState::Unused {
            return SbiRet::invalid_state();
        }
        event.handler_pc = handler_entry_pc;
        event.handler_arg = handler_entry_arg;
        event.pending = false;
        event.state = EventState::Registered;
        SbiRet::success(0)
    }

    fn unregister(&self, event_id: u32) -> SbiRet {
        let Some(idx) = event_index(event_id) else {
            return SbiRet::not_supported();
        };
        let mut events = EVENTS[current_hart()].lock();
        let event = &mut events[idx];
        match event.state {
            EventState::Registered | EventState::Disabled => {
                event.handler_pc = 0;
                event.handler_arg = 0;
                event.pending = false;
                event.state = EventState::Unused;
                SbiRet::success(0)
            }
            _ => SbiRet::invalid_state(),
        }
    }

    fn enable(&self, event_id: u32) -> SbiRet {
        let Some(idx) = event_index(event_id) else {
            return SbiRet::not_supported();
        };
        let mut events = EVENTS[current_hart()].lock();
        let event = &mut events[idx];
        match event.state {
            EventState::Registered | EventState::Disabled => {
                event.state = EventState::Enabled;
                SbiRet::success(0)
            }
            _ => SbiRet::invalid_state(),
        }
    }

    fn disable(&self, event_id: u32) -> SbiRet {
        let Some(idx) = event_index(event_id) else {
            return SbiRet::not_supported();
        };
        let mut events = EVENTS[current_hart()].lock();
        let event = &mut events[idx];
        match event.state {
            EventState::Enabled | EventState::Running => {
                event.state = EventState::Disabled;
                event.pending = false;
                SbiRet::success(0)
            }
            _ => SbiRet::invalid_state(),
        }
    }

    fn complete(&self) -> SbiRet {
        let mut events = EVENTS[current_hart()].lock();
        for event in events.iter_mut() {
            if event.state == EventState::Running {
                event.state = EventState::Enabled;
                event.pending = false;
                return SbiRet::success(0);
            }
        }
        SbiRet::invalid_state()
    }

    fn inject(&self, event_id: u32, hart_id: usize) -> SbiRet {
        let Some(idx) = event_index(event_id) else {
            return SbiRet::not_supported();
        };
        let Some(events) = EVENTS.get(hart_id) else {
            return SbiRet::invalid_param();
        };
        let mut events = events.lock();
        let event = &mut events[idx];
        if event.state != EventState::Enabled {
            return SbiRet::invalid_state();
        }
        event.pending = true;
        if !HART_MASKED[hart_id].load(Ordering::Acquire) {
            event.state = EventState::Running;
        }
        SbiRet::success(0)
    }

    fn hart_unmask(&self) -> SbiRet {
        let hart_id = current_hart();
        HART_MASKED[hart_id].store(false, Ordering::Release);
        let mut events = EVENTS[hart_id].lock();
        if let Some(event) = events
            .iter_mut()
            .filter(|event| event.state == EventState::Enabled && event.pending)
            .min_by_key(|event| event.priority)
        {
            event.state = EventState::Running;
        }
        SbiRet::success(0)
    }

    fn hart_mask(&self) -> SbiRet {
        HART_MASKED[current_hart()].store(true, Ordering::Release);
        SbiRet::success(0)
    }
}
