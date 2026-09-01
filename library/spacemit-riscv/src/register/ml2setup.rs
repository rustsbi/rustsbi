//! Machine L2 setup register.
//!
//! # Platform support
//!
//! This register is supported on SpacemiT X60, X100, and A100 cores. K1
//! firmware accesses the core-slot snoop fields; K3 firmware also accesses
//! `iprf` and `tprf` on both K3 core families.
//!
//! Field meanings are inferred from public K1 and K3 firmware. K1 and K3 use
//! bits 3:0 for a hart's core slot within a four-core cluster; bits 16 and 18
//! are observed only in K3 firmware.
//!
//! [K1 OpenSBI source](https://github.com/riscv-software-src/opensbi/blob/fb70fe8b98c0e6ca23201bf7014ae85a3ef692b9/lib/utils/hsm/fdt_hsm_spacemit.c#L90-L109)
//! [K3 OpenSBI source](https://github.com/spacemit-com/opensbi/blob/8bd2cbdf9856dbc1a990d36e26bf47411f356c42/platform/generic/spacemit/spacemit_k3.c#L217-L233)

use riscv::result::{Error, Result};

riscv::read_write_csr! {
    /// Machine L2 setup register.
    Ml2setup: 0x7f0,
    mask: 0x0005_000f,
}

riscv::read_write_csr_field! {
    Ml2setup,
    /// Core-slot snoop enable.
    ///
    /// This meaning is inferred from K1 and K3 hart-index operations.
    snoop_enable: 0..=3,
}

riscv::read_write_csr_field! {
    Ml2setup,
    /// L2 instruction-fetch-miss cache-line prefetch enable.
    ///
    /// This meaning is inferred from K3 firmware.
    iprf: 16,
}

riscv::read_write_csr_field! {
    Ml2setup,
    /// L2 TLB prefetch enable.
    ///
    /// This meaning is inferred from K3 firmware.
    tprf: 18,
}

riscv::set!(0x7f0);
riscv::clear!(0x7f0);

/// Enables snooping for a core slot in the range zero through three.
///
/// **WARNING**: panics on non-RISC-V targets or if `index` is out of bounds.
///
/// # Safety
///
/// The current hart must implement this SpacemiT CSR, and changing its snoop
/// state must be valid for the platform's cache-coherency sequence.
#[inline]
pub unsafe fn set_snoop_enable(index: usize) {
    unsafe { try_set_snoop_enable(index) }.unwrap();
}

/// Attempts to enable snooping for a core slot in the range zero through three.
///
/// # Safety
///
/// The current hart must implement this SpacemiT CSR, and changing its snoop
/// state must be valid for the platform's cache-coherency sequence.
#[inline]
pub unsafe fn try_set_snoop_enable(index: usize) -> Result<()> {
    if index < 4 {
        unsafe { _try_set(1 << index) }
    } else {
        Err(Error::IndexOutOfBounds {
            index,
            min: 0,
            max: 3,
        })
    }
}

/// Disables snooping for a core slot in the range zero through three.
///
/// **WARNING**: panics on non-RISC-V targets or if `index` is out of bounds.
///
/// # Safety
///
/// The current hart must implement this SpacemiT CSR, and changing its snoop
/// state must be valid for the platform's cache-coherency sequence.
#[inline]
pub unsafe fn clear_snoop_enable(index: usize) {
    unsafe { try_clear_snoop_enable(index) }.unwrap();
}

/// Attempts to disable snooping for a core slot in the range zero through three.
///
/// # Safety
///
/// The current hart must implement this SpacemiT CSR, and changing its snoop
/// state must be valid for the platform's cache-coherency sequence.
#[inline]
pub unsafe fn try_clear_snoop_enable(index: usize) -> Result<()> {
    if index < 4 {
        unsafe { _try_clear(1 << index) }
    } else {
        Err(Error::IndexOutOfBounds {
            index,
            min: 0,
            max: 3,
        })
    }
}

riscv::set_clear_csr!(
    /// L2 instruction-fetch-miss cache-line prefetch enable.
    ///
    /// This meaning is inferred from K3 firmware.
    , set_iprf, clear_iprf, 1 << 16);
riscv::set_clear_csr!(
    /// L2 TLB prefetch enable.
    ///
    /// This meaning is inferred from K3 firmware.
    , set_tprf, clear_tprf, 1 << 18);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ml2setup_fields() {
        let mut value = Ml2setup::from_bits(0);
        assert_eq!(Ml2setup::BITMASK, 0x0005_000f);

        for index in 0..4 {
            assert!(!value.snoop_enable(index));
            value.set_snoop_enable(index, true);
            assert!(value.snoop_enable(index));
        }

        value.set_iprf(true);
        value.set_tprf(true);
        assert!(value.iprf());
        assert!(value.tprf());
        assert_eq!(value.bits(), 0x0005_000f);
    }

    #[test]
    fn ml2setup_snoop_index_errors() {
        let mut value = Ml2setup::from_bits(0);
        let expected = Error::IndexOutOfBounds {
            index: 4,
            min: 0,
            max: 3,
        };

        assert_eq!(value.try_snoop_enable(4), Err(expected));
        assert_eq!(value.try_set_snoop_enable(4, true), Err(expected));
    }
}
