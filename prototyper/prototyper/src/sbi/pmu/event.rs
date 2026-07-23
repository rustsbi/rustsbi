//! Validation of SBI PMU event encodings.

use rustsbi::SbiRet;
use sbi_spec::pmu::{
    cache_event, cache_operation, cache_result, event_type, firmware_event, hardware_event,
};

const EVENT_INDEX_MASK: usize = 0x000f_ffff;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct Event {
    pub(super) index: usize,
    pub(super) selector: u64,
    pub(super) kind: EventKind,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum EventKind {
    Hardware,
    Firmware(usize),
}

impl Event {
    pub(super) fn parse(index: usize, data: u64) -> Result<Self, SbiRet> {
        if index & !EVENT_INDEX_MASK != 0 {
            return Err(SbiRet::invalid_param());
        }
        let kind = (index >> 16) & 0xf;
        let code = index & 0xffff;
        match kind {
            event_type::HARDWARE_GENERAL
                if (hardware_event::CPU_CYCLES..=hardware_event::REF_CPU_CYCLES)
                    .contains(&code) =>
            {
                Ok(Self {
                    index,
                    selector: index as u64,
                    kind: EventKind::Hardware,
                })
            }
            event_type::HARDWARE_CACHE if valid_cache_event(code) => Ok(Self {
                index,
                selector: index as u64,
                kind: EventKind::Hardware,
            }),
            event_type::HARDWARE_RAW | event_type::HARDWARE_RAW_V2 if code == 0 => Ok(Self {
                index,
                selector: data,
                kind: EventKind::Hardware,
            }),
            event_type::FIRMWARE if supported_firmware_event(code) && data == 0 => Ok(Self {
                index,
                selector: 0,
                kind: EventKind::Firmware(code),
            }),
            event_type::FIRMWARE
                if code <= firmware_event::HFENCE_VVMA_ASID_RECEIVED
                    || code == firmware_event::PLATFORM =>
            {
                Err(SbiRet::not_supported())
            }
            event_type::HARDWARE_GENERAL
            | event_type::HARDWARE_CACHE
            | event_type::HARDWARE_RAW
            | event_type::HARDWARE_RAW_V2
            | event_type::FIRMWARE => Err(SbiRet::invalid_param()),
            _ => Err(SbiRet::invalid_param()),
        }
    }
}

fn valid_cache_event(code: usize) -> bool {
    let cache = (code >> 3) & 0x1fff;
    let operation = (code >> 1) & 0x3;
    let result = code & 1;
    cache <= cache_event::NODE
        && operation <= cache_operation::PREFETCH
        && result <= cache_result::MISS
}

fn supported_firmware_event(code: usize) -> bool {
    let base = matches!(
        code,
        firmware_event::MISALIGNED_LOAD
            | firmware_event::MISALIGNED_STORE
            | firmware_event::ILLEGAL_INSN
            | firmware_event::SET_TIMER
            | firmware_event::IPI_SENT
            | firmware_event::FENCE_I_SENT
            | firmware_event::SFENCE_VMA_SENT
            | firmware_event::SFENCE_VMA_ASID_SENT
    );
    #[cfg(feature = "hypervisor")]
    {
        base || matches!(
            code,
            firmware_event::HFENCE_GVMA_SENT
                | firmware_event::HFENCE_GVMA_VMID_SENT
                | firmware_event::HFENCE_VVMA_SENT
                | firmware_event::HFENCE_VVMA_ASID_SENT
        )
    }
    #[cfg(not(feature = "hypervisor"))]
    base
}
