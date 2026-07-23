//! Private versioned contract linked by the Prototyper development kit.

use crate::boot::NextMode;

const MAGIC: u32 = 0x5054_5950;
const VERSION: u16 = 1;
const FW_DYNAMIC: u8 = 1;
const FW_JUMP: u8 = 2;
const FW_PAYLOAD: u8 = 3;

#[repr(C, align(8))]
struct Contract {
    magic: u32,
    version: u16,
    firmware_type: u8,
    next_mode: u8,
    flags: u32,
    reserved: u32,
    next_address: u64,
    allowed_start: u64,
    allowed_end: u64,
}

unsafe extern "C" {
    static __prototyper_contract_start: u8;
    static __prototyper_contract_end: u8;
    static __prototyper_dtb_start: u8;
    static __prototyper_dtb_end: u8;
    static __prototyper_payload_start: u8;
    static __prototyper_payload_end: u8;
}

pub(super) fn fixed_stage() -> Option<(usize, NextMode)> {
    let contract = contract()?;
    let mode = match contract.next_mode {
        0 => NextMode::User,
        1 => NextMode::Supervisor,
        3 => NextMode::Machine,
        _ => return None,
    };
    let address = usize::try_from(contract.next_address).ok()?;
    if !next_address_allowed(address) {
        return None;
    }
    match contract.firmware_type {
        FW_JUMP if payload_range().is_none() => Some((address, mode)),
        FW_PAYLOAD => {
            let payload = payload_range()?;
            (address == payload.0).then_some((address, mode))
        }
        _ => None,
    }
}

pub(super) fn selected_dtb(provider_address: usize) -> usize {
    linked_range(
        core::ptr::addr_of!(__prototyper_dtb_start) as usize,
        core::ptr::addr_of!(__prototyper_dtb_end) as usize,
    )
    .map_or(provider_address, |range| range.0)
}

pub(crate) fn next_address_allowed(address: usize) -> bool {
    let Some(contract) = contract() else {
        return false;
    };
    let Ok(address) = u64::try_from(address) else {
        return false;
    };
    contract.allowed_start < contract.allowed_end
        && (contract.allowed_start..contract.allowed_end).contains(&address)
}

fn contract() -> Option<&'static Contract> {
    let start = core::ptr::addr_of!(__prototyper_contract_start) as usize;
    let end = core::ptr::addr_of!(__prototyper_contract_end) as usize;
    if end.checked_sub(start)? != core::mem::size_of::<Contract>()
        || !start.is_multiple_of(core::mem::align_of::<Contract>())
    {
        return None;
    }
    // SAFETY: the linker script fixes the section size and alignment. The
    // bytes are immutable for the complete firmware lifetime.
    let contract = unsafe { &*(start as *const Contract) };
    (contract.magic == MAGIC
        && contract.version == VERSION
        && contract.flags == 0
        && contract.reserved == 0
        && matches!(contract.firmware_type, FW_DYNAMIC | FW_JUMP | FW_PAYLOAD))
    .then_some(contract)
}

fn payload_range() -> Option<(usize, usize)> {
    linked_range(
        core::ptr::addr_of!(__prototyper_payload_start) as usize,
        core::ptr::addr_of!(__prototyper_payload_end) as usize,
    )
}

fn linked_range(start: usize, end: usize) -> Option<(usize, usize)> {
    (start < end).then_some((start, end))
}

const _: () = assert!(core::mem::size_of::<Contract>() == 40);
