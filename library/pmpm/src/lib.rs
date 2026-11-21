//! PMP slot management.
//!
//! A PMP slot refers to the collection of PMP entries that correspond to the same index
//! across all harts, used to provide a unified memory isolation view for all harts in Penglai/Keystone.
//! The module consists of two parts: bitmap-based PMP slot management and software interrupt-based
//! PMP synchronization implementation.
#![no_std]
pub const MAX_PMP_ENTRY_COUNT: u32 = 16;
pub const PMP_SHIFT: u32 = 2;

/// A PMP slot index. Always below `MAX_PMP_ENTRY_COUNT`, so out-of-range values cannot be built.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PmpSlot(u32);

impl PmpSlot {
    /// Returns the slot for `idx`, or `None` if `idx` is out of range.
    pub const fn new(idx: u32) -> Option<Self> {
        if idx < MAX_PMP_ENTRY_COUNT {
            Some(PmpSlot(idx))
        } else {
            None
        }
    }

    /// The raw PMP entry index.
    pub const fn index(self) -> u32 {
        self.0
    }
}

use core::usize;

use riscv::register::{
    Permission, Range, pmpaddr0, pmpaddr1, pmpaddr2, pmpaddr3, pmpaddr4, pmpaddr5, pmpaddr6,
    pmpaddr7, pmpaddr8, pmpaddr9, pmpaddr10, pmpaddr11, pmpaddr12, pmpaddr13, pmpaddr14, pmpaddr15,
    pmpcfg0, pmpcfg2,
};
#[cfg(target_arch = "riscv32")]
use riscv::register::{pmpcfg1, pmpcfg3};
pub mod bitmap;

#[derive(Debug, Clone, Copy)]
pub struct PmpConfig {
    range: Range,
    perm: Permission,
    is_locked: bool,
}

impl PmpConfig {
    pub fn new(range: Range, perm: Permission, is_locked: bool) -> Self {
        Self {
            range,
            perm,
            is_locked,
        }
    }
    pub fn range(&self) -> Range {
        self.range
    }
    pub fn perm(&self) -> Permission {
        self.perm
    }
    pub fn is_locked(&self) -> bool {
        self.is_locked
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PmpSlice {
    pa_lo: usize,
    pa_hi: usize,
    log2len: u32,
}

impl PmpSlice {
    pub fn new(log2len: u32, pa_lo: usize, pa_hi: usize) -> Self {
        PmpSlice {
            log2len,
            pa_lo,
            pa_hi,
        }
    }
    pub fn log2len(&self) -> u32 {
        self.log2len
    }
    /// Region length. Saturates at `usize::MAX` for the full address space (`log2len == BITS`).
    pub fn len(&self) -> usize {
        if self.log2len >= usize::BITS {
            usize::MAX
        } else {
            1usize << self.log2len
        }
    }
    pub fn lo(&self) -> usize {
        self.pa_lo
    }
    /// Exclusive upper bound of a TOR region. Only meaningful for `Range::TOR`.
    pub fn hi(&self) -> usize {
        self.pa_hi
    }
    pub fn start(&self) -> usize {
        self.pa_lo
    }
    /// Last address in the region. Saturates for the full address space.
    pub fn end(&self) -> usize {
        self.pa_lo.saturating_add(self.len() - 1)
    }
}

#[inline]
/// Memory region check was expect to execute before set PMP regs, to simplify PMP ops, PMPM
/// request user check memory manual
pub fn check_pmp_area_available(addr: usize, len: usize, range: Range) -> bool {
    match range {
        Range::NA4 => (addr & ((1 << PMP_SHIFT) - 1)) == 0 && len == 4,
        Range::NAPOT => len >= 8 && (len & (len - 1) == 0) && (addr % len == 0),
        Range::TOR => (addr & ((1 << PMP_SHIFT) - 1)) == 0 && len % 4 == 0,
        _ => true,
    }
}

// TODO: Under 32-bit architecture, both length and address may overflow and need to be fixed
// Encode addr to PMP addr.
pub fn encode_pmp_addr(slice: &PmpSlice, range: Range) -> usize {
    let mut addr = slice.pa_lo;
    let log2len = slice.log2len & (usize::BITS | (usize::BITS - 1));
    match range {
        Range::NAPOT => {
            if log2len == usize::BITS {
                usize::MAX
            } else {
                let addrmask = (1usize << (log2len - PMP_SHIFT)) - 1;
                addr = (addr >> PMP_SHIFT) & !addrmask;
                addr | (addrmask >> 1)
            }
        }
        Range::NA4 => addr >> PMP_SHIFT,
        // TOR: pmpaddr[i] holds the exclusive top; pmpaddr[i-1] holds the start (set by the caller).
        Range::TOR => slice.pa_hi >> PMP_SHIFT,
        Range::OFF => 0,
    }
}

// TODO: Under 32-bit architecture, both length and address may overflow and need to be fixed
// Decode addr from PMP addr.
pub fn decode_pmp_addr(pmpaddr: usize, range: Range) -> PmpSlice {
    let mut addr = pmpaddr;
    match range {
        Range::NAPOT => {
            if pmpaddr == usize::MAX {
                PmpSlice::new(usize::BITS, 0, 0)
            } else {
                let mut log2len = (!pmpaddr).trailing_zeros();
                addr = (addr & !((1usize << log2len) - 1)) << PMP_SHIFT;
                log2len = log2len + PMP_SHIFT + 1;
                PmpSlice::new(log2len, addr, 0)
            }
        }
        Range::NA4 => PmpSlice::new(2, pmpaddr << PMP_SHIFT, 0),
        // A single TOR register holds only the top; the start lives in pmpaddr[i-1].
        Range::TOR => PmpSlice::new(0, 0, pmpaddr << PMP_SHIFT),
        Range::OFF => PmpSlice::new(0, 0, 0),
    }
}

/// RV32: each config register holds 4 entries, so entries map to pmpcfg0..3.
#[cfg(target_arch = "riscv32")]
pub fn set_pmp_cfg(slot: PmpSlot, config: &PmpConfig) {
    let idx = slot.index();
    unsafe {
        match idx {
            0..=3 => pmpcfg0::set_pmp(idx as usize, config.range, config.perm, config.is_locked),
            4..=7 => pmpcfg1::set_pmp(
                (idx - 4) as usize,
                config.range,
                config.perm,
                config.is_locked,
            ),
            8..=11 => pmpcfg2::set_pmp(
                (idx - 8) as usize,
                config.range,
                config.perm,
                config.is_locked,
            ),
            12..=15 => pmpcfg3::set_pmp(
                (idx - 12) as usize,
                config.range,
                config.perm,
                config.is_locked,
            ),
            _ => panic!("Invalid PMP index for cfg"),
        }
    }
}

/// RV64: pmpcfg0 holds entries 0-7 and pmpcfg2 holds 8-15.
#[cfg(not(target_arch = "riscv32"))]
pub fn set_pmp_cfg(slot: PmpSlot, config: &PmpConfig) {
    let idx = slot.index();
    let cfg_idx = (idx % 8) as usize;
    unsafe {
        match idx {
            0..=7 => pmpcfg0::set_pmp(cfg_idx, config.range, config.perm, config.is_locked),
            8..=15 => pmpcfg2::set_pmp(cfg_idx, config.range, config.perm, config.is_locked),
            _ => panic!("Invalid PMP index for cfg"),
        }
    }
}

unsafe fn _set_pmp_addr(idx: u32, addr: usize) {
    unsafe {
        match idx {
            0 => pmpaddr0::write(addr),
            1 => pmpaddr1::write(addr),
            2 => pmpaddr2::write(addr),
            3 => pmpaddr3::write(addr),
            4 => pmpaddr4::write(addr),
            5 => pmpaddr5::write(addr),
            6 => pmpaddr6::write(addr),
            7 => pmpaddr7::write(addr),
            8 => pmpaddr8::write(addr),
            9 => pmpaddr9::write(addr),
            10 => pmpaddr10::write(addr),
            11 => pmpaddr11::write(addr),
            12 => pmpaddr12::write(addr),
            13 => pmpaddr13::write(addr),
            14 => pmpaddr14::write(addr),
            15 => pmpaddr15::write(addr),
            _ => panic!("Invalid PMP index for addr"),
        }
    }
}

//TODO: It depends on IPIs to sync PMP configuration, here is dummy implement.
pub fn set_pmp_entry_sync(slot: PmpSlot, addr: usize, len: usize, config: &PmpConfig) -> bool {
    set_pmp_entry(slot, addr, len, config)
}

/// Returns the log2 of `len` for `range`, or `None` if NAPOT is given a length below 8 bytes.
///
/// Only NAPOT uses the length. Other ranges ignore it, so `len` (possibly 0) is accepted.
/// Lengths below 8 would underflow the NAPOT encoding. Non-power-of-two lengths round down;
/// the caller must check the area first (see `check_pmp_area_available`).
///
/// Contract for full-space NAPOT: `len == usize::MAX` means the entire implemented address
/// space. It encodes to an all-ones pmpaddr, and decoding that value gives back `usize::BITS`.
/// Coverage of the real hardware depends on the implemented physical address width.
fn slice_log2len(len: usize, range: Range) -> Option<u32> {
    match range {
        Range::NAPOT => {
            if len == usize::MAX {
                Some(usize::BITS)
            } else if len >= 8 {
                len.checked_ilog2()
            } else {
                None
            }
        }
        Range::NA4 | Range::TOR | Range::OFF => Some(0),
    }
}

/// Builds the slice to encode for `range`. Returns `None` if the length cannot be represented
/// (NAPOT below 8 bytes, or a TOR top that overflows).
fn pmp_slice(addr: usize, len: usize, range: Range) -> Option<PmpSlice> {
    match range {
        // TOR: the top boundary is addr + len. pmpaddr[i-1] must be set to `addr` by the caller.
        Range::TOR => Some(PmpSlice::new(0, addr, addr.checked_add(len)?)),
        _ => Some(PmpSlice::new(slice_log2len(len, range)?, addr, 0)),
    }
}

/// For TOR, entry `slot` covers [pmpaddr[slot-1], pmpaddr[slot]). Accept `addr` only if it is
/// that bottom, since this function does not write pmpaddr[slot-1].
fn tor_bottom_matches(slot: PmpSlot, addr: usize) -> bool {
    if slot.index() == 0 {
        addr == 0
    } else {
        // Compare in pmpaddr units: shifting the register left would drop its top bits on RV32.
        _get_pmp_addr(slot.index() - 1) == (addr >> PMP_SHIFT)
    }
}

pub fn set_pmp_entry(slot: PmpSlot, addr: usize, len: usize, config: &PmpConfig) -> bool {
    let idx = slot.index();
    // The memory area should be check before function call.
    if get_pmp_cfg(slot).is_locked() {
        return false;
    }
    if !pmpaddr_writable(slot) {
        return false;
    }
    if config.range == Range::TOR && !tor_bottom_matches(slot, addr) {
        return false;
    }
    let Some(slice) = pmp_slice(addr, len, config.range) else {
        return false;
    };
    unsafe {
        _set_pmp_addr(idx, encode_pmp_addr(&slice, config.range));
        set_pmp_cfg(slot, config);
    }
    true
}

pub fn set_pmp_addr(slot: PmpSlot, addr: usize, len: usize, range: Range) -> bool {
    let idx = slot.index();
    // The memory area should be check before function call.
    if !pmpaddr_writable(slot) {
        return false;
    }
    if range == Range::TOR && !tor_bottom_matches(slot, addr) {
        return false;
    }
    let Some(slice) = pmp_slice(addr, len, range) else {
        return false;
    };
    unsafe {
        _set_pmp_addr(idx, encode_pmp_addr(&slice, range));
    }
    true
}

/// Whether writing pmpaddr[slot] is allowed. Refused if the entry is locked (hardware would
/// ignore the write), or if the next entry is a TOR entry, whether locked or not: its bottom is
/// pmpaddr[slot], so writing it would silently move that entry's region. Integrators must not
/// give slots that sit directly below a TOR chain (such as the prototyper's low-slot TOR setup)
/// to this crate's protector.
fn pmpaddr_writable(slot: PmpSlot) -> bool {
    if get_pmp_cfg(slot).is_locked() {
        return false;
    }
    if let Some(next_slot) = PmpSlot::new(slot.index() + 1) {
        if get_pmp_cfg(next_slot).range() == Range::TOR {
            return false;
        }
    }
    true
}

/// Turns the entry off by writing its config only. Its pmpaddr is kept, because it is the
/// bottom of a TOR entry at the next slot.
pub fn disable_pmp_entry(slot: PmpSlot) -> bool {
    if get_pmp_cfg(slot).is_locked() {
        return false;
    }
    set_pmp_cfg(slot, &PmpConfig::new(Range::OFF, Permission::NONE, false));
    true
}

/// Sync variant of `disable_pmp_entry`, used by the protector. Mirrors `set_pmp_entry_sync`.
//TODO: It depends on IPIs to sync PMP configuration, here is dummy implement.
pub fn disable_pmp_entry_sync(slot: PmpSlot) -> bool {
    disable_pmp_entry(slot)
}

fn _get_pmp_addr(idx: u32) -> usize {
    match idx {
        0 => pmpaddr0::read(),
        1 => pmpaddr1::read(),
        2 => pmpaddr2::read(),
        3 => pmpaddr3::read(),
        4 => pmpaddr4::read(),
        5 => pmpaddr5::read(),
        6 => pmpaddr6::read(),
        7 => pmpaddr7::read(),
        8 => pmpaddr8::read(),
        9 => pmpaddr9::read(),
        10 => pmpaddr10::read(),
        11 => pmpaddr11::read(),
        12 => pmpaddr12::read(),
        13 => pmpaddr13::read(),
        14 => pmpaddr14::read(),
        15 => pmpaddr15::read(),
        _ => panic!("Invalid PMP index"),
    }
}

/// Reads the raw config for entry `idx`. Mirrors the register mapping in `set_pmp_cfg`.
#[cfg(target_arch = "riscv32")]
fn read_pmp_cfg(idx: u32) -> riscv::register::Pmp {
    match idx {
        0..=3 => pmpcfg0::read().into_config(idx as usize),
        4..=7 => pmpcfg1::read().into_config((idx - 4) as usize),
        8..=11 => pmpcfg2::read().into_config((idx - 8) as usize),
        12..=15 => pmpcfg3::read().into_config((idx - 12) as usize),
        _ => panic!("Invalid PMP index"),
    }
}

#[cfg(not(target_arch = "riscv32"))]
fn read_pmp_cfg(idx: u32) -> riscv::register::Pmp {
    let cfg_idx = (idx % 8) as usize;
    match idx {
        0..=7 => pmpcfg0::read().into_config(cfg_idx),
        8..=15 => pmpcfg2::read().into_config(cfg_idx),
        _ => panic!("Invalid PMP index"),
    }
}

pub fn get_pmp_cfg(slot: PmpSlot) -> PmpConfig {
    let pmp_config = read_pmp_cfg(slot.index());
    PmpConfig {
        range: (pmp_config.range),
        perm: (pmp_config.permission),
        is_locked: (pmp_config.locked),
    }
}

pub fn get_pmp_entry(slot: PmpSlot) -> (usize, PmpConfig) {
    let idx = slot.index();
    (_get_pmp_addr(idx), get_pmp_cfg(slot))
}

#[cfg(test)]
mod tests {
    use super::*;
    use riscv::register::Range;

    fn create_pmp_slice(log2len: u32, pa_lo: usize) -> PmpSlice {
        PmpSlice::new(log2len, pa_lo, 0)
    }

    #[test]
    fn test_encode_decode_napot() {
        let full_slice = create_pmp_slice(usize::BITS, 0x0);
        let full_encoded = encode_pmp_addr(&full_slice, Range::NAPOT);
        assert_eq!(
            full_encoded,
            usize::MAX,
            "NAPOT full address space encode failed"
        );
        let full_decoded = decode_pmp_addr(full_encoded, Range::NAPOT);
        assert_eq!(
            full_decoded.log2len(),
            usize::BITS,
            "NAPOT full address space decode log2len failed"
        );
        assert_eq!(
            full_decoded.start(),
            0x0,
            "NAPOT full address space decode start address failed"
        );

        let log2len_64kb = 16;
        let pa_lo_64kb = 0x10000;
        let slice_64kb = create_pmp_slice(log2len_64kb, pa_lo_64kb);
        let expected_64kb = 0x5FFF;
        let encoded_64kb = encode_pmp_addr(&slice_64kb, Range::NAPOT);
        assert_eq!(
            encoded_64kb, expected_64kb,
            "NAPOT 64KB region encode failed"
        );
        let decoded_64kb = decode_pmp_addr(encoded_64kb, Range::NAPOT);
        assert_eq!(
            decoded_64kb.log2len(),
            log2len_64kb,
            "NAPOT 64KB region decode log2len failed"
        );
        assert_eq!(
            decoded_64kb.start(),
            pa_lo_64kb,
            "NAPOT 64KB region decode start address failed"
        );

        let log2len_2mb = 21;
        let pa_lo_2mb = 0x400000;
        let slice_2mb = create_pmp_slice(log2len_2mb, pa_lo_2mb);
        let expected_2mb = 0x100000 | ((1usize << 19) - 1) >> 1;
        let encoded_2mb = encode_pmp_addr(&slice_2mb, Range::NAPOT);
        assert_eq!(encoded_2mb, expected_2mb, "NAPOT 2MB region encode failed");
        let decoded_2mb = decode_pmp_addr(encoded_2mb, Range::NAPOT);
        assert_eq!(
            decoded_2mb.log2len(),
            log2len_2mb,
            "NAPOT 2MB region decode log2len failed"
        );
        assert_eq!(
            decoded_2mb.start(),
            pa_lo_2mb,
            "NAPOT 2MB region decode start address failed"
        );
    }

    #[test]
    fn test_encode_decode_na4() {
        let pa_lo_4byte = 0x12345678;
        let slice_4byte = create_pmp_slice(PMP_SHIFT, pa_lo_4byte);
        let expected_encoded = pa_lo_4byte >> PMP_SHIFT;
        let encoded_4byte = encode_pmp_addr(&slice_4byte, Range::NA4);
        assert_eq!(encoded_4byte, expected_encoded, "NA4 mode encode failed");
        let decoded_4byte = decode_pmp_addr(encoded_4byte, Range::NA4);
        assert_eq!(
            decoded_4byte.log2len(),
            PMP_SHIFT,
            "NA4 mode decode log2len failed"
        );
        assert_eq!(
            decoded_4byte.start(),
            pa_lo_4byte,
            "NA4 mode decode start address failed"
        );
        assert_eq!(decoded_4byte.len(), 4, "NA4 mode decode length failed");

        let random_encoded = 0xABCDEF;
        let decoded_random = decode_pmp_addr(random_encoded, Range::NA4);
        assert_eq!(
            decoded_random.len(),
            4,
            "NA4 mode decode should return fixed 4 bytes length"
        );
    }

    #[test]
    fn test_encode_decode_tor() {
        // Region [0x8000_0000, 0x8000_1400): length 0x1400 is not a power of two.
        let slice_tor = pmp_slice(0x8000_0000, 0x1400, Range::TOR).unwrap();
        let encoded_tor = encode_pmp_addr(&slice_tor, Range::TOR);
        assert_eq!(
            encoded_tor,
            0x8000_1400 >> PMP_SHIFT,
            "TOR encode should write the top"
        );
        let decoded_tor = decode_pmp_addr(encoded_tor, Range::TOR);
        assert_eq!(
            decoded_tor.hi(),
            0x8000_1400,
            "TOR decode should return the top"
        );
        assert_eq!(
            decoded_tor.log2len(),
            0,
            "TOR decode should return log2len=0"
        );

        // Overflowing top is rejected instead of wrapping.
        assert!(pmp_slice(usize::MAX - 3, 8, Range::TOR).is_none());
    }

    #[test]
    fn test_encode_decode_off() {
        let random_slice = create_pmp_slice(10, 0xFFFFFFFF);
        let encoded_off = encode_pmp_addr(&random_slice, Range::OFF);
        assert_eq!(encoded_off, 0, "OFF mode encode should return 0");

        let decoded_off = decode_pmp_addr(0, Range::OFF);
        assert_eq!(
            decoded_off.start(),
            0,
            "OFF mode decode start address should return 0"
        );
        assert_eq!(
            decoded_off.log2len(),
            0,
            "OFF mode decode log2len should return 0"
        );
        assert_eq!(
            decoded_off.len(),
            1 << 0,
            "OFF mode decode length should return 1"
        );
    }

    #[test]
    fn test_slice_log2len() {
        assert_eq!(slice_log2len(8, Range::NAPOT), Some(3));
        assert_eq!(slice_log2len(0x1000, Range::NAPOT), Some(12));
        assert_eq!(slice_log2len(usize::MAX, Range::NAPOT), Some(usize::BITS));
        // Zero length for NAPOT (what enable() passes) must not panic.
        assert_eq!(slice_log2len(0, Range::NAPOT), None);
        // Lengths below 8 would underflow the NAPOT encoding.
        assert_eq!(slice_log2len(4, Range::NAPOT), None);
        // Other ranges ignore the length, so zero is fine (disable() relies on this).
        assert_eq!(slice_log2len(0, Range::OFF), Some(0));
        assert_eq!(slice_log2len(0, Range::NA4), Some(0));
        assert_eq!(slice_log2len(0, Range::TOR), Some(0));
    }

    #[test]
    fn test_pmp_slot_bounds() {
        assert_eq!(PmpSlot::new(0).map(|s| s.index()), Some(0));
        assert_eq!(
            PmpSlot::new(MAX_PMP_ENTRY_COUNT - 1).map(|s| s.index()),
            Some(15)
        );
        assert!(PmpSlot::new(MAX_PMP_ENTRY_COUNT).is_none());
        assert!(PmpSlot::new(u32::MAX).is_none());
    }

    #[test]
    fn test_full_space_napot() {
        // Full-space NAPOT (len == usize::MAX) encodes to all ones and decodes back to BITS.
        let full = slice_for_full_space();
        assert_eq!(encode_pmp_addr(&full, Range::NAPOT), usize::MAX);
        assert_eq!(
            decode_pmp_addr(usize::MAX, Range::NAPOT).log2len(),
            usize::BITS
        );
    }

    fn slice_for_full_space() -> PmpSlice {
        create_pmp_slice(usize::BITS, 0)
    }

    #[test]
    fn test_check_pmp_area_available() {
        assert!(
            !check_pmp_area_available(0x12345679, 4, Range::NA4),
            "Address not 4-byte aligned should be invalid"
        );
        assert!(
            !check_pmp_area_available(0x12345678, 2, Range::TOR),
            "Length less than 4 bytes should be invalid"
        );

        assert!(
            check_pmp_area_available(0x12345678, 4, Range::NA4),
            "NA4 mode 4-byte region should be valid"
        );
        assert!(
            !check_pmp_area_available(0x12345678, 8, Range::NA4),
            "NA4 mode length greater than 4 should be invalid"
        );

        assert!(
            check_pmp_area_available(0x10000, 65536, Range::NAPOT),
            "NAPOT mode 64KB region should be valid"
        );
        assert!(
            !check_pmp_area_available(0x10000, 65535, Range::NAPOT),
            "NAPOT mode length not power of 2 should be invalid"
        );
        assert!(
            !check_pmp_area_available(0x10001, 65536, Range::NAPOT),
            "NAPOT mode address not aligned should be invalid"
        );
        assert!(
            !check_pmp_area_available(0x10000, 4, Range::NAPOT),
            "NAPOT mode length less than 8 should be invalid"
        );

        assert!(
            check_pmp_area_available(0x80000000, 1024, Range::TOR),
            "TOR mode 1024-byte region should be valid"
        );
        assert!(
            !check_pmp_area_available(0x80000000, 1023, Range::TOR),
            "TOR mode length not 4-byte aligned should be invalid"
        );

        assert!(
            check_pmp_area_available(0x0, 0, Range::OFF),
            "OFF mode should always be valid"
        );
        assert!(
            check_pmp_area_available(0x1, 1, Range::OFF),
            "OFF mode should always be valid"
        );
    }

    #[test]
    fn test_pmp_config() {
        let range = Range::NAPOT;
        let perm = Permission::RW;
        let is_locked = true;
        let pmp_cfg = PmpConfig::new(range, perm, is_locked);

        assert_eq!(pmp_cfg.range(), range, "PmpConfig range attribute mismatch");
        assert_eq!(pmp_cfg.perm(), perm, "PmpConfig perm attribute mismatch");
        assert_eq!(
            pmp_cfg.is_locked(),
            is_locked,
            "PmpConfig is_locked attribute mismatch"
        );
    }

    #[test]
    fn test_pmp_slice() {
        let log2len = 10;
        let pa_lo = 0x1000;
        let slice = create_pmp_slice(log2len, pa_lo);

        assert_eq!(slice.log2len(), log2len, "PmpSlice log2len mismatch");
        assert_eq!(
            slice.len(),
            1 << log2len,
            "PmpSlice length calculation error"
        );
        assert_eq!(slice.start(), pa_lo, "PmpSlice start address mismatch");
        assert_eq!(
            slice.end(),
            pa_lo + (1 << log2len) - 1,
            "PmpSlice end address calculation error"
        );
    }
}
