//! Typed PMP encoding values and validated capability facts.

use alloc::vec::Vec;

pub(super) const MAX_PMP_ENTRIES: usize = 64;

bitflags::bitflags! {
    /// Lower-privilege access granted by one PMP entry.
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub(super) struct Permissions: u8 {
        const READ = 1 << 0;
        const WRITE = 1 << 1;
        const EXECUTE = 1 << 2;
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub(super) enum AddressMode {
    NaturallyAlignedFourBytes = 2 << 3,
    NaturallyAlignedPowerOfTwo = 3 << 3,
}

impl AddressMode {
    pub(super) const fn bits(self) -> u8 {
        self as u8
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct Region {
    pub(super) start: usize,
    pub(super) end: usize,
}

impl Region {
    pub(super) fn new(start: usize, end: usize) -> Result<Self, PmpError> {
        if start >= end {
            return Err(PmpError::InvalidRegion);
        }
        Ok(Self { start, end })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct Capability {
    pub(super) entries: usize,
    pub(super) granularity: usize,
    pub(super) napot_address_mask: usize,
}

impl Capability {
    pub(super) fn new(
        entries: usize,
        granularity: usize,
        napot_address_mask: usize,
    ) -> Result<Self, PmpError> {
        let address_mask_is_contiguous = napot_address_mask == usize::MAX
            || napot_address_mask
                .checked_add(1)
                .is_some_and(usize::is_power_of_two);
        if entries > MAX_PMP_ENTRIES
            || granularity < 4
            || !granularity.is_power_of_two()
            || (entries == 0 && napot_address_mask != 0)
            || (entries != 0 && (!address_mask_is_contiguous || napot_address_mask == 0))
        {
            return Err(PmpError::InvalidCapability);
        }
        Ok(Self {
            entries,
            granularity,
            napot_address_mask,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct Entry {
    pub(super) address: usize,
    pub(super) permissions: Permissions,
    pub(super) mode: AddressMode,
}

impl Entry {
    pub(super) const fn config_byte(self) -> u8 {
        self.permissions.bits() | self.mode.bits()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum Image {
    Protected(Vec<Entry>),
    TrustedWithoutPmp,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PmpError {
    InvalidRegion,
    InvalidCapability,
    AddressOutOfRange,
    Unrepresentable,
    InsufficientEntries,
    PmpRequired,
    LockedState,
    ExtendedState,
    HardwareUnavailable,
    UnexpectedFault,
    InconsistentCapability,
    VerificationFailed,
}
