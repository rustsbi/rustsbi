//! PMP entry allocation and hart-local programming.
#![no_std]
pub const MAX_PMP_ENTRY_COUNT: u32 = 16;
pub const PMP_SHIFT: u32 = 2;

use riscv::register::{
    Permission, Range, pmpaddr0, pmpaddr1, pmpaddr2, pmpaddr3, pmpaddr4, pmpaddr5, pmpaddr6,
    pmpaddr7, pmpaddr8, pmpaddr9, pmpaddr10, pmpaddr11, pmpaddr12, pmpaddr13, pmpaddr14, pmpaddr15,
    pmpcfg0, pmpcfg2,
};
pub mod bitmap;

#[derive(Debug, Clone, Copy, PartialEq)]
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
        assert!(log2len <= usize::BITS + 3 && pa_hi < 4);
        PmpSlice {
            log2len,
            pa_lo,
            pa_hi,
        }
    }
    pub fn log2len(&self) -> u32 {
        self.log2len
    }
    pub fn len(&self) -> u128 {
        1u128 << self.log2len
    }
    pub fn lo(&self) -> usize {
        self.pa_lo
    }
    pub fn start(&self) -> u128 {
        (self.pa_hi as u128) << usize::BITS | self.pa_lo as u128
    }
    pub fn end(&self) -> u128 {
        self.start() + self.len() - 1
    }
}

/// Checks the addressing mode's alignment and size requirements.
#[inline]
pub fn check_pmp_area_available(addr: usize, len: usize, range: Range) -> bool {
    match range {
        Range::NA4 => (addr & ((1 << PMP_SHIFT) - 1)) == 0 && len == 4,
        Range::NAPOT => {
            (addr == 0 && len == usize::MAX)
                || (len >= 8 && len.is_power_of_two() && addr.is_multiple_of(len))
        }
        Range::TOR => (addr & ((1 << PMP_SHIFT) - 1)) == 0 && len % 4 == 0,
        _ => true,
    }
}

/// Encodes the address register. For TOR, the address is the exclusive upper bound.
pub fn encode_pmp_addr(slice: &PmpSlice, range: Range) -> usize {
    let address = (slice.start() >> PMP_SHIFT) as usize;
    match range {
        Range::NAPOT => {
            assert!(slice.log2len >= 3 && slice.start() & (slice.len() - 1) == 0);
            address | ((slice.len() - 1) >> 3) as usize
        }
        Range::NA4 | Range::TOR => address,
        Range::OFF => 0,
    }
}

/// Decodes one address register. For TOR, the result identifies the upper bound.
pub fn decode_pmp_addr(pmpaddr: usize, range: Range) -> PmpSlice {
    let address = (pmpaddr as u128) << PMP_SHIFT;
    let (start, log2len) = match range {
        Range::NAPOT => {
            let log2len = pmpaddr.trailing_ones() + 3;
            (address & !((1u128 << log2len) - 1), log2len)
        }
        Range::NA4 => (address, 2),
        Range::TOR => (address, 0),
        Range::OFF => (0, 0),
    };
    PmpSlice::new(log2len, start as usize, (start >> usize::BITS) as usize)
}

// RV64 packs eight entries into each even-numbered register. RV32 packs four
// into each consecutive register.
macro_rules! with_pmpcfg {
    ($idx:expr, |$csr:ident, $byte:ident| $operation:expr) => {{
        let index = $idx as usize;
        let entries = size_of::<usize>();
        let $byte = index % entries;
        match index / entries * (entries / 4) {
            0 => {
                use pmpcfg0 as $csr;
                $operation
            }
            #[cfg(target_arch = "riscv32")]
            1 => {
                use riscv::register::pmpcfg1 as $csr;
                $operation
            }
            2 => {
                use pmpcfg2 as $csr;
                $operation
            }
            #[cfg(target_arch = "riscv32")]
            3 => {
                use riscv::register::pmpcfg3 as $csr;
                $operation
            }
            _ => panic!("PMP index out of range"),
        }
    }};
}

/// Writes the configuration byte of an implemented PMP entry.
///
/// # Safety
///
/// Execute in M-mode with exclusive access to the packed CSR. The new
/// permissions must allow all live accesses, including M-mode accesses
/// affected by locking the entry. Synchronize translations before resuming
/// contexts that rely on the new permissions.
pub unsafe fn set_pmp_cfg(idx: u32, config: &PmpConfig) {
    // SAFETY: The caller owns the packed CSR and permits this configuration change.
    with_pmpcfg!(idx, |csr, byte| unsafe {
        csr::set_pmp(byte, config.range, config.perm, config.is_locked)
    });
}

/// # Safety
///
/// Own the writable entry on this hart in M-mode. Exclude accesses affected
/// by the address change, including those matched by a following TOR entry.
unsafe fn write_address(idx: u32, addr: usize) {
    // SAFETY: Each arm writes only the address CSR reserved by the caller.
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

fn encoded_address(addr: usize, len: usize, range: Range) -> Option<usize> {
    if !check_pmp_area_available(addr, len, range) {
        return None;
    }
    Some(match range {
        Range::NAPOT => {
            let log2len = if len == usize::MAX {
                usize::BITS + 3
            } else {
                len.ilog2()
            };
            encode_pmp_addr(&PmpSlice::new(log2len, addr, 0), range)
        }
        Range::TOR | Range::NA4 => addr >> PMP_SHIFT,
        Range::OFF => 0,
    })
}

/// Programs one entry on the current hart and checks its effective settings.
/// A NAPOT range of `(0, usize::MAX)` covers all implemented physical addresses.
/// OFF preserves the address register, which may bound the next TOR entry.
/// A readback mismatch triggers rollback. The caller's `synchronize` callback
/// runs once after programming or rollback and before the result is returned.
///
/// # Safety
///
/// Execute in M-mode. Own the implemented, writable entry and its packed CSR.
/// Use standard PMP permissions. Keep lower-mode accesses and any accesses
/// relying on an adjacent TOR boundary quiescent until this operation completes.
/// The entry can temporarily select NAPOT while its address is saved. The new
/// permissions, including a requested lock, must be valid for the platform.
/// `synchronize` must order PMP updates according to this hart's capabilities.
/// Paging requires `SFENCE.VMA x0, x0`, and H additionally requires
/// `HFENCE.GVMA x0, x0`. A hart without paging or H requires neither.
/// See [PMP and paging](https://docs.riscv.org/reference/isa/v20260120/priv/machine.html#pmp-vmem).
pub unsafe fn set_pmp_entry(
    idx: u32,
    addr: usize,
    len: usize,
    config: &PmpConfig,
    synchronize: impl FnOnce(),
) -> bool {
    if idx >= MAX_PMP_ENTRY_COUNT {
        return false;
    }
    let Some(encoded) = encoded_address(addr, len, config.range) else {
        return false;
    };
    let old_config = get_pmp_cfg(idx);
    if old_config.is_locked {
        return false;
    }
    let unlocked = PmpConfig {
        is_locked: false,
        ..*config
    };
    // SAFETY: The caller owns the entry and has excluded affected accesses.
    let success = unsafe {
        let napot = PmpConfig::new(Range::NAPOT, Permission::NONE, false);
        let saved_address = if config.range == Range::OFF {
            None
        } else {
            // OFF and TOR hide a retained address bit on coarse-grained harts.
            // NAPOT exposes every bit that can affect a later match.
            if matches!(old_config.range, Range::OFF | Range::TOR) {
                set_pmp_cfg(idx, &napot);
            }
            let saved = read_address(idx);
            write_address(idx, encoded);
            Some(saved)
        };
        set_pmp_cfg(idx, &unlocked);
        let (actual, effective) = get_pmp_entry(idx);
        let address_matches = config.range == Range::OFF
            || actual == encoded
            || (encoded == usize::MAX && actual != 0 && actual & actual.wrapping_add(1) == 0);
        let mut accepted = effective == unlocked && address_matches;
        if accepted && config.is_locked {
            set_pmp_cfg(idx, config);
            accepted = get_pmp_cfg(idx) == *config;
        }
        if !accepted {
            if let Some(saved) = saved_address {
                set_pmp_cfg(idx, &napot);
                write_address(idx, saved);
            }
            set_pmp_cfg(idx, &old_config);
        }
        accepted
    };
    synchronize();
    success
}

/// Writes only the address register. TOR uses `addr` as its upper bound.
/// Returns whether the request has a valid encoding and index. OFF preserves
/// the address. Verify the effective address after selecting the final range
/// mode, or use [`set_pmp_entry`] to program and verify both together.
///
/// # Safety
///
/// The caller must own the implemented, writable entry and any adjacent TOR
/// boundary, exclude affected accesses, and synchronize translations after programming.
pub unsafe fn set_pmp_addr(idx: u32, addr: usize, len: usize, range: Range) -> bool {
    let Some(encoded) = encoded_address(addr, len, range) else {
        return false;
    };
    if idx >= MAX_PMP_ENTRY_COUNT {
        return false;
    }
    if range == Range::OFF {
        return true;
    }
    // SAFETY: The caller owns the address CSR and excludes affected accesses.
    unsafe { write_address(idx, encoded) };
    true
}

fn read_address(idx: u32) -> usize {
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

pub fn get_pmp_cfg(idx: u32) -> PmpConfig {
    let pmp_config = with_pmpcfg!(idx, |csr, byte| csr::read().into_config(byte));
    PmpConfig {
        range: (pmp_config.range),
        perm: (pmp_config.permission),
        is_locked: (pmp_config.locked),
    }
}

pub fn get_pmp_entry(idx: u32) -> (usize, PmpConfig) {
    (read_address(idx), get_pmp_cfg(idx))
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
        let full_slice = create_pmp_slice(usize::BITS + 3, 0x0);
        let full_encoded = encode_pmp_addr(&full_slice, Range::NAPOT);
        assert_eq!(
            full_encoded,
            usize::MAX,
            "NAPOT full address space encode failed"
        );
        let full_decoded = decode_pmp_addr(full_encoded, Range::NAPOT);
        assert_eq!(
            full_decoded.log2len(),
            usize::BITS + 3,
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
            pa_lo_64kb as u128,
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
            pa_lo_2mb as u128,
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
            pa_lo_4byte as u128,
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
        let pa_lo_tor = 0x80000000;
        let slice_tor = create_pmp_slice(30, pa_lo_tor);
        let expected_encoded = pa_lo_tor >> PMP_SHIFT;
        let encoded_tor = encode_pmp_addr(&slice_tor, Range::TOR);
        assert_eq!(encoded_tor, expected_encoded, "TOR mode encode failed");
        let decoded_tor = decode_pmp_addr(encoded_tor, Range::TOR);
        assert_eq!(
            decoded_tor.start(),
            pa_lo_tor as u128,
            "TOR mode decode start address failed"
        );
        assert_eq!(
            decoded_tor.log2len(),
            0,
            "TOR mode decode should return log2len=0"
        );

        let pa_lo_align = 0x1000;
        let encoded_align = encode_pmp_addr(&create_pmp_slice(0, pa_lo_align), Range::TOR);
        let decoded_align = decode_pmp_addr(encoded_align, Range::TOR);
        assert_eq!(
            decoded_align.start(),
            pa_lo_align as u128,
            "TOR mode aligned address decode failed"
        );
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
        assert_eq!(
            slice.start(),
            pa_lo as u128,
            "PmpSlice start address mismatch"
        );
        assert_eq!(
            slice.end(),
            pa_lo as u128 + (1 << log2len) - 1,
            "PmpSlice end address calculation error"
        );
    }
}
