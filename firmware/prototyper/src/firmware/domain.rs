//! PMP regions used to isolate platform memory.

use riscv::register::{Permission, Range, pmpcfg0, pmpcfg2};
use riscv::register::{
    pmpaddr0, pmpaddr1, pmpaddr2, pmpaddr3, pmpaddr4, pmpaddr5, pmpaddr6, pmpaddr7, pmpaddr8,
    pmpaddr9, pmpaddr10, pmpaddr11, pmpaddr12, pmpaddr13, pmpaddr14, pmpaddr15,
};

/// Deny all R/W/X access in all modes and lock the PMP entry.
pub const ENF_PERMISSIONS: u32 = 1 << 0;

/// M-mode R/W/X, S/U denied; the PMP entry is not locked (M-mode access is
/// what allows M-mode to emulate S-mode accesses, e.g. the K3
/// REGISTER_PRESERVATION window).
pub const M_RWX: u32 = 1 << 1;

pub struct DomainRegion {
    base: usize,
    size: usize,
    flags: u32,
}

impl DomainRegion {
    pub const fn new(base: usize, size: usize, flags: u32) -> Self {
        Self { base, size, flags }
    }

    fn end(&self) -> Option<usize> {
        self.base.checked_add(self.size)
    }
}

/// Programs a run of protected windows inside `[region_start, region_end)`
/// into PMP TOR entries beginning at `first_slot`.
///
/// Windows must be nonempty, ordered, nonoverlapping, and contained in the
/// supplied physical range.
pub fn program_windows(
    first_slot: usize,
    region_start: usize,
    region_end: usize,
    windows: &[DomainRegion],
) -> Option<usize> {
    let entries = windows.len().checked_mul(2)?.checked_add(1)?;
    if first_slot.checked_add(entries)? > 16 || region_start > region_end {
        return None;
    }
    let mut cursor = region_start;
    for window in windows {
        let end = window.end()?;
        if window.size == 0 || window.base < cursor || end > region_end {
            return None;
        }
        cursor = end;
    }

    let mut slot = first_slot;
    for w in windows {
        write_entry(slot, Permission::RWX, false, w.base);
        slot += 1;
        let locked = w.flags & ENF_PERMISSIONS != 0;
        write_entry(slot, Permission::NONE, locked, w.end()?);
        slot += 1;
    }
    write_entry(slot, Permission::RWX, false, region_end);
    Some(slot + 1)
}

/// Writes a single PMP TOR entry at `slot` (0..=15).
///
/// # Panics
///
/// Panics if `slot >= 16`.
pub fn write_entry(slot: usize, perm: Permission, locked: bool, addr: usize) {
    assert!(slot < 16, "PMP slot {slot} out of range");
    let idx = slot & 7;
    // SAFETY: PMP CSR accesses are volatile register writes; `slot` is
    // bounds-checked above and the addresses are validated by callers.
    unsafe {
        match slot {
            0..=7 => pmpcfg0::set_pmp(idx, Range::TOR, perm, locked),
            8..=15 => pmpcfg2::set_pmp(idx, Range::TOR, perm, locked),
            _ => unreachable!(),
        }
    }
    let addr = addr >> 2;
    // SAFETY: the slot is bounds-checked and the address is caller-validated.
    unsafe {
        match slot {
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
            _ => unreachable!(),
        }
    }
}
