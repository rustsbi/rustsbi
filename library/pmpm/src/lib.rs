//! PMP slot management.
//!
//! A PMP slot refers to the collection of PMP entries that correspond to the same index
//! across all harts, used to provide a unified memory isolation view for all harts in Penglai/Keystone.
//! The module consists of two parts: bitmap-based PMP slot management and software interrupt-based
//! PMP synchronization implementation.
#![no_std]

use core::sync::atomic::{AtomicUsize, Ordering};
use riscv::register::{
    Permission, Pmp, Range, pmpaddr0, pmpaddr1, pmpaddr2, pmpaddr3, pmpaddr4, pmpaddr5, pmpaddr6,
    pmpaddr7, pmpaddr8, pmpaddr9, pmpaddr10, pmpaddr11, pmpaddr12, pmpaddr13, pmpaddr14, pmpaddr15,
    pmpcfg0, pmpcfg2,
};

// PMP management bitmap, for alloc/free PMP slot in Penglai/Keystone. A bitmap
// can describe a segment of consecutive PMP entries.
pub struct PmpBitmap {
    map: AtomicUsize,
    start: u8,
    end: u8,
}

#[allow(unused)]
impl PmpBitmap {
    pub const fn new(pmp_start: u8, pmp_end: u8, mask: usize) -> Self {
        Self {
            map: (AtomicUsize::new(mask)),
            start: (pmp_start),
            end: (pmp_end),
        }
    }
    pub fn alloc(&self) -> Option<u8> {
        let mut cur_map = self.map.load(Ordering::Acquire);
        loop {
            // Find idx of first free slot in current PMP area and set to used.
            // Beware that reserved PMP slot will set to used when TEE init.
            let first_free = (self.start..self.end).find(|&i| cur_map & (1 << i) == 0)?;
            let new_map = cur_map | (1 << first_free);

            // Try to update bitmap by CAS. If update successfully, then return idx.
            match self.map.compare_exchange_weak(
                cur_map,
                new_map,
                Ordering::SeqCst,
                Ordering::SeqCst,
            ) {
                Ok(_) => return Some(first_free),
                Err(actual) => cur_map = actual,
            }
        }
    }
    pub fn free(&self, idx: u8, mask: usize) {
        self.map.fetch_and(!((1 << idx) & mask), Ordering::SeqCst);
    }
    pub fn region(&self) -> (u8, u8) {
        (self.start, self.end)
    }
    pub fn get(&self) -> usize {
        self.map.load(Ordering::SeqCst)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct PmpSlice {
    num_bytes: usize,
    pa_lo: usize,
    pa_hi: usize,
}

impl PmpSlice {
    pub fn new(num_bytes: usize, pa_lo: usize, pa_hi: usize) -> Self {
        PmpSlice {
            num_bytes,
            pa_lo,
            pa_hi,
        }
    }
    #[inline]
    pub fn size(&self) -> usize {
        self.num_bytes
    }
    pub fn lo(&self) -> usize {
        self.pa_lo
    }
    pub fn hi(&self) -> usize {
        self.pa_hi
    }
}

#[inline]
fn check_pmp_region(slice: PmpSlice) -> bool {
    let len = slice.num_bytes;
    let addr = slice.pa_lo;
    // len must be power of 2 and no less than 4, addr must be aligned to len
    if len < 4 || len & (len - 1) != 0 || addr & (len - 1) != 0 {
        return false;
    }
    true
}

// TODO: Under 32-bit architecture, both length and address may overflow and need to be fixed
// Encode addr to PMP addr.
pub fn encode_pmp_addr(slice: PmpSlice, mode: Range) -> Option<usize> {
    let addr = slice.pa_lo;
    let len = slice.num_bytes;
    match mode {
        Range::NAPOT => {
            if check_pmp_region(slice) {
                Some((addr | ((len >> 1) - 1)) >> 2)
            } else {
                None
            }
        }
        Range::NA4 => Some(addr >> 2),
        Range::TOR => Some(addr >> 2),
        Range::OFF => Some(0),
    }
}
// TODO: Under 32-bit architecture, both length and address may overflow and need to be fixed
// Decode addr from PMP addr.
pub fn decode_pmp_addr(pmp_addr: usize, mode: Range) -> PmpSlice {
    let mut addr = pmp_addr;
    match mode {
        Range::NAPOT => {
            let order = addr.trailing_ones();
            addr &= !((1 << (order + 1)) - 1);
            PmpSlice {
                pa_lo: addr << 2,
                pa_hi: addr.rotate_left(2) & 0b11,
                num_bytes: 1 << (order + 3),
            }
        }
        Range::NA4 => PmpSlice::new(4, pmp_addr << 2, pmp_addr.rotate_left(2) & 0b11),
        Range::TOR => PmpSlice::new(0, pmp_addr << 2, pmp_addr.rotate_left(2) & 0b11),
        Range::OFF => PmpSlice::new(0, 0, 0),
    }
}

#[allow(unused)]
#[inline]
/// Get PMP entry @idx on local hart.
pub fn get_pmp_entry(idx: u8) -> (PmpSlice, Pmp) {
    let (pmp_addr, pmp_config) = get_pmp_reg(idx);
    (decode_pmp_addr(pmp_addr, pmp_config.range), pmp_config)
}

#[allow(unused)]
#[inline]
/// Set PMP entry @idx on local hart.
pub fn set_pmp_entry(idx: u8, slice: PmpSlice, mode: Range, perm: Permission) {
    if let Some(pmp_addr) = encode_pmp_addr(slice, mode) {
        set_pmp_reg(idx, pmp_addr, mode, perm)
    }
}

fn set_pmp_reg(idx: u8, pmp_addr: usize, mode: Range, perm: Permission) {
    unsafe {
        match idx {
            0 => {
                pmpaddr0::write(pmp_addr);
                pmpcfg0::set_pmp(0, mode, perm, false);
            }
            1 => {
                pmpaddr1::write(pmp_addr);
                pmpcfg0::set_pmp(1, mode, perm, false);
            }
            2 => {
                pmpaddr2::write(pmp_addr);
                pmpcfg0::set_pmp(2, mode, perm, false);
            }
            3 => {
                pmpaddr3::write(pmp_addr);
                pmpcfg0::set_pmp(3, mode, perm, false);
            }
            4 => {
                pmpaddr4::write(pmp_addr);
                pmpcfg0::set_pmp(4, mode, perm, false);
            }
            5 => {
                pmpaddr5::write(pmp_addr);
                pmpcfg0::set_pmp(5, mode, perm, false);
            }
            6 => {
                pmpaddr6::write(pmp_addr);
                pmpcfg0::set_pmp(6, mode, perm, false);
            }
            7 => {
                pmpaddr7::write(pmp_addr);
                pmpcfg0::set_pmp(7, mode, perm, false);
            }
            8 => {
                pmpaddr8::write(pmp_addr);
                pmpcfg2::set_pmp(0, mode, perm, false);
            }
            9 => {
                pmpaddr9::write(pmp_addr);
                pmpcfg2::set_pmp(1, mode, perm, false);
            }
            10 => {
                pmpaddr10::write(pmp_addr);
                pmpcfg2::set_pmp(2, mode, perm, false);
            }
            11 => {
                pmpaddr11::write(pmp_addr);
                pmpcfg2::set_pmp(3, mode, perm, false);
            }
            12 => {
                pmpaddr12::write(pmp_addr);
                pmpcfg2::set_pmp(4, mode, perm, false);
            }
            13 => {
                pmpaddr13::write(pmp_addr);
                pmpcfg2::set_pmp(5, mode, perm, false);
            }
            14 => {
                pmpaddr14::write(pmp_addr);
                pmpcfg2::set_pmp(6, mode, perm, false);
            }
            _ => {
                pmpaddr15::write(pmp_addr);
                pmpcfg2::set_pmp(7, mode, perm, false);
            }
        }
    }
}

fn get_pmp_reg(idx: u8) -> (usize, Pmp) {
    match idx {
        0 => (pmpaddr0::read(), pmpcfg0::read().into_config(0)),
        1 => (pmpaddr1::read(), pmpcfg0::read().into_config(1)),
        2 => (pmpaddr2::read(), pmpcfg0::read().into_config(2)),
        3 => (pmpaddr3::read(), pmpcfg0::read().into_config(3)),
        4 => (pmpaddr4::read(), pmpcfg0::read().into_config(4)),
        5 => (pmpaddr5::read(), pmpcfg0::read().into_config(5)),
        6 => (pmpaddr6::read(), pmpcfg0::read().into_config(6)),
        7 => (pmpaddr7::read(), pmpcfg0::read().into_config(7)),
        8 => (pmpaddr8::read(), pmpcfg2::read().into_config(0)),
        9 => (pmpaddr9::read(), pmpcfg2::read().into_config(1)),
        10 => (pmpaddr10::read(), pmpcfg2::read().into_config(2)),
        11 => (pmpaddr11::read(), pmpcfg2::read().into_config(3)),
        12 => (pmpaddr12::read(), pmpcfg2::read().into_config(4)),
        13 => (pmpaddr13::read(), pmpcfg2::read().into_config(5)),
        14 => (pmpaddr14::read(), pmpcfg2::read().into_config(6)),
        _ => (pmpaddr15::read(), pmpcfg2::read().into_config(7)),
    }
}
