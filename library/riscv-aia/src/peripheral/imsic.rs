//! Incoming MSI Controller (IMSIC) peripheral.

use volatile_register::WO;

/// Incoming MSI Controller (IMSIC) register block.
///
/// Each interrupt file in an IMSIC has one or two memory-mapped 32-bit
/// registers for receiving MSI writes. These memory-mapped registers are
/// located within a naturally aligned 4-KiB region (a page) of physical
/// address space that exists for the interrupt file.
///
/// The rest of the 4-KiB page is reserved and read-only zeros.
#[repr(C)]
pub struct Imsic {
    /// 0x000 - Set interrupt-pending bit by number, little-endian.
    pub seteipnum_le: WO<u32>,
    /// 0x004 - Set interrupt-pending bit by number, big-endian.
    pub seteipnum_be: WO<u32>,
}

impl Imsic {
    /// Returns the size of the IMSIC register block.
    pub const fn size() -> usize {
        0x1000 // 4-KiB page
    }
}

/// External interrupt delivery enable register (`eidelivery`).
///
/// This register controls whether interrupts from this interrupt file are
/// delivered from the IMSIC to the attached hart.
///
/// *NOTE:* Guest interrupt files do not support value 0x40000000 for `eidelivery`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct Eidelivery(u32);

impl Eidelivery {
    /// Interrupt delivery is disabled.
    pub const DISABLED: Self = Self(0);

    /// Interrupt delivery from the interrupt file is enabled.
    pub const ENABLED: Self = Self(1);

    /// Interrupt delivery from a PLIC or APLIC is enabled (optional).
    pub const PLIC_APLIC_ENABLED: Self = Self(0x40000000);

    /// Creates a new `Eidelivery` from a raw value.
    pub const fn from_raw(value: u32) -> Self {
        Self(value)
    }

    /// Returns the raw value of this register.
    pub const fn raw(self) -> u32 {
        self.0
    }

    /// Returns whether interrupt delivery is disabled.
    pub const fn is_disabled(self) -> bool {
        self.0 == Self::DISABLED.0
    }

    /// Returns whether interrupt delivery from the interrupt file is enabled.
    pub const fn is_enabled(self) -> bool {
        self.0 == Self::ENABLED.0
    }

    /// Returns whether interrupt delivery from a PLIC or APLIC is enabled.
    pub const fn is_plic_aplic_enabled(self) -> bool {
        self.0 == Self::PLIC_APLIC_ENABLED.0
    }
}

/// External interrupt threshold register (`eithreshold`).
///
/// This register specifies the minimum priority level for an interrupt
/// to be delivered. Interrupts with priority below this threshold are masked.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct Eithreshold(u32);

impl Eithreshold {
    /// Creates a new `Eithreshold` from a raw value.
    pub const fn from_raw(value: u32) -> Self {
        Self(value)
    }

    /// Returns the raw value of this register.
    pub const fn raw(self) -> u32 {
        self.0
    }

    /// Returns the threshold value.
    pub const fn threshold(self) -> u32 {
        self.0
    }

    /// Returns the priority threshold.
    pub const fn priority(self) -> u8 {
        (self.0 >> 24) as u8
    }
}

/// External interrupt-pending bits register (`eip[n]`).
///
/// Each bit represents whether the corresponding interrupt identity is pending.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct Eip(u32);

impl Eip {
    /// Creates a new `Eip` from a raw value.
    pub const fn from_raw(value: u32) -> Self {
        Self(value)
    }

    /// Returns the raw value of this register.
    pub const fn raw(self) -> u32 {
        self.0
    }

    /// Returns whether the interrupt with the given index (0-31) is pending.
    pub const fn is_pending(self, index: u32) -> bool {
        if index >= 32 {
            return false;
        }
        (self.0 & (1 << index)) != 0
    }

    /// Sets the pending status for the interrupt with the given index (0-31).
    pub const fn set_pending(mut self, index: u32, pending: bool) -> Self {
        if index >= 32 {
            return self;
        }
        if pending {
            self.0 |= 1 << index;
        } else {
            self.0 &= !(1 << index);
        }
        self
    }
}

/// External interrupt-enable bits register (`eie[n]`).
///
/// Each bit represents whether the corresponding interrupt identity is enabled.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct Eie(u32);

impl Eie {
    /// Creates a new `Eie` from a raw value.
    pub const fn from_raw(value: u32) -> Self {
        Self(value)
    }

    /// Returns the raw value of this register.
    pub const fn raw(self) -> u32 {
        self.0
    }

    /// Returns whether the interrupt with the given index (0-31) is enabled.
    pub const fn is_enabled(self, index: u32) -> bool {
        if index >= 32 {
            return false;
        }
        (self.0 & (1 << index)) != 0
    }

    /// Sets the enable status for the interrupt with the given index (0-31).
    pub const fn set_enabled(mut self, index: u32, enabled: bool) -> Self {
        if index >= 32 {
            return self;
        }
        if enabled {
            self.0 |= 1 << index;
        } else {
            self.0 &= !(1 << index);
        }
        self
    }
}

/// Top external interrupt register (`topei`).
///
/// This register returns the interrupt identity and priority of the highest-priority
/// pending and enabled interrupt, and simultaneously claims (clears the pending bit)
/// for that interrupt.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct Topei(u32);

impl Topei {
    /// No interrupt pending.
    pub const NONE: Self = Self(0);

    /// Creates a new `Topei` from a raw value.
    pub const fn from_raw(value: u32) -> Self {
        Self(value)
    }

    /// Returns the raw value of this register.
    pub const fn raw(self) -> u32 {
        self.0
    }

    /// Returns the interrupt identity (bits 31:16).
    pub const fn interrupt_identity(self) -> u16 {
        (self.0 >> 16) as u16
    }

    /// Returns the interrupt priority (bits 10:0).
    pub const fn priority(self) -> u16 {
        (self.0 & 0x7FF) as u16
    }

    /// Returns whether an interrupt is pending.
    pub const fn is_pending(self) -> bool {
        self.0 != 0
    }
}

// Constants for indirect register selection
pub mod select {
    /// External interrupt delivery enable register.
    pub const EIDELIVERY: u32 = 0x70;

    /// External interrupt threshold register.
    pub const EITHRESHOLD: u32 = 0x72;

    /// External interrupt-pending bits registers (eip0 to eip63).
    pub const EIP_BASE: u32 = 0x80;

    /// External interrupt-enable bits registers (eie0 to eie63).
    pub const EIE_BASE: u32 = 0xC0;
}

/// Maximum interrupt identity number supported by IMSIC.
pub const MAX_INTERRUPT_IDENTITY: u16 = 2047;

/// Minimum valid interrupt identity number.
// *NOTE:* Interrupt identity 0 is never valid.
// Lower-numbered interrupt identities have higher priority than higher-numbered ones.
pub const MIN_INTERRUPT_IDENTITY: u16 = 1;

/// MSI (Message-Signaled Interrupt) encoding utilities.
pub mod msi {
    use super::{MAX_INTERRUPT_IDENTITY, MIN_INTERRUPT_IDENTITY};

    /// Encodes an interrupt identity into MSI data.
    ///
    /// Returns the 32-bit MSI data value for the given interrupt identity in little-endian byte order.
    #[inline]
    pub const fn encode_le(identity: u16) -> u32 {
        identity as u32
    }

    /// Returns the 32-bit MSI data value for the given interrupt identity in big-endian byte order.
    #[inline]
    pub const fn encode_be(identity: u16) -> u32 {
        (identity as u32) << 16
    }

    /// Decodes little-endian MSI data into an interrupt identity, returning `None` if invalid.
    #[inline]
    pub const fn decode_le(data: u32) -> Option<u16> {
        let identity = data as u16;
        if identity >= MIN_INTERRUPT_IDENTITY && identity <= MAX_INTERRUPT_IDENTITY {
            Some(identity)
        } else {
            None
        }
    }

    /// Decodes big-endian MSI data into an interrupt identity, returning `None` if invalid.
    #[inline]
    pub const fn decode_be(data: u32) -> Option<u16> {
        let identity = ((data >> 16) & 0xFFFF) as u16;
        if identity >= MIN_INTERRUPT_IDENTITY && identity <= MAX_INTERRUPT_IDENTITY {
            Some(identity)
        } else {
            None
        }
    }
}

/// Interrupt file operation utilities.
pub mod file_ops {
    use super::{Eie, Eip, MAX_INTERRUPT_IDENTITY, select};

    /// Returns `(register_index, bit_position)` for the given interrupt identity.
    #[inline]
    pub const fn identity_to_register(identity: u16) -> Option<(u32, u32)> {
        if identity == 0 || identity > MAX_INTERRUPT_IDENTITY {
            return None;
        }
        let reg_index = ((identity - 1) / 32) as u32;
        let bit_pos = ((identity - 1) % 32) as u32;
        Some((reg_index, bit_pos))
    }

    /// Calculates the interrupt identity from register index and bit position.
    #[inline]
    pub const fn register_to_identity(reg_index: u32, bit_pos: u32) -> Option<u16> {
        if reg_index > 63 || bit_pos > 31 {
            return None;
        }
        let identity = (reg_index * 32 + bit_pos + 1) as u16;
        if identity <= MAX_INTERRUPT_IDENTITY {
            Some(identity)
        } else {
            None
        }
    }

    /// Gets the select value for an eip register.
    #[inline]
    pub const fn eip_select(reg_index: u32) -> u32 {
        select::EIP_BASE + reg_index
    }

    /// Gets the select value for an eie register.
    #[inline]
    pub const fn eie_select(reg_index: u32) -> u32 {
        select::EIE_BASE + reg_index
    }

    /// Checks if an interrupt identity is valid.
    #[inline]
    pub const fn is_valid_identity(identity: u16) -> bool {
        identity >= 1 && identity <= MAX_INTERRUPT_IDENTITY
    }

    /// Iterator over all valid interrupt identities.
    pub struct IdentityIterator {
        current: u16,
    }

    impl IdentityIterator {
        /// Creates a new iterator over all valid interrupt identities.
        pub const fn new() -> Self {
            Self { current: 1 }
        }
    }

    impl Iterator for IdentityIterator {
        type Item = u16;

        fn next(&mut self) -> Option<Self::Item> {
            if self.current <= MAX_INTERRUPT_IDENTITY {
                let result = self.current;
                self.current += 1;
                Some(result)
            } else {
                None
            }
        }
    }

    /// Bulk operations on interrupt pending/enable arrays.
    pub mod bulk {
        use super::*;

        /// Sets multiple interrupt pending bits at once.
        ///
        /// `identities` should contain the interrupt identities to set.
        /// Invalid identities are ignored.
        ///
        /// Returns an array of (register_index, eip_value) pairs to update.
        /// Only registers that have updates are included.
        pub fn set_pending_batch<const N: usize>(identities: &[u16; N]) -> [(u32, Eip); 64] {
            let mut updates = [(0, Eip::from_raw(0)); 64];

            for &identity in identities {
                if let Some((reg_idx, bit_pos)) = identity_to_register(identity) {
                    if reg_idx < 64 {
                        let eip = &mut updates[reg_idx as usize].1;
                        updates[reg_idx as usize].0 = reg_idx;
                        *eip = eip.set_pending(bit_pos, true);
                    }
                }
            }

            updates
        }

        /// Sets multiple interrupt enable bits at once.
        pub fn set_enable_batch<const N: usize>(identities: &[u16; N]) -> [(u32, Eie); 64] {
            let mut updates = [(0, Eie::from_raw(0)); 64];

            for &identity in identities {
                if let Some((reg_idx, bit_pos)) = identity_to_register(identity) {
                    if reg_idx < 64 {
                        let eie = &mut updates[reg_idx as usize].1;
                        updates[reg_idx as usize].0 = reg_idx;
                        *eie = eie.set_enabled(bit_pos, true);
                    }
                }
            }

            updates
        }
    }
}

/// System-level IMSIC configuration and address calculation.
pub mod system {
    /// IMSIC address layout parameters.
    ///
    /// IMSIC interrupt files are arranged in memory according to specific patterns
    /// to allow efficient address calculation.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct AddressLayout {
        /// Base address for machine-level interrupt files.
        pub machine_base: usize,
        /// Base address for supervisor-level interrupt files.
        pub supervisor_base: usize,
        /// Base address for guest interrupt files (if supported).
        pub guest_base: Option<usize>,
        /// Number of bits for hart index (j parameter).
        pub hart_index_bits: u32,
        /// Number of bits for group index (E parameter).
        pub group_bits: u32,
        /// Number of bits for hart offset within group (C parameter).
        pub hart_offset_bits: u32,
        /// Number of bits for guest file offset (D parameter).
        pub guest_offset_bits: u32,
    }

    impl AddressLayout {
        /// Creates a default address layout following the AIA specification.
        pub const fn default() -> Self {
            Self {
                machine_base: 0x2800_0000,
                supervisor_base: 0x2800_4000,
                guest_base: Some(0x2800_8000),
                hart_index_bits: 12,
                group_bits: 24,
                hart_offset_bits: 12,
                guest_offset_bits: 12,
            }
        }

        /// Calculates the address of a machine-level interrupt file.
        ///
        /// # Parameters
        /// - `hart_id`: The hart ID
        /// - `group_id`: The group ID (usually 0 for single-group systems)
        pub const fn machine_interrupt_file_address(&self, hart_id: u32, group_id: u32) -> usize {
            self.machine_base
                + (group_id << self.group_bits) as usize
                + (hart_id << self.hart_offset_bits) as usize
        }

        /// Calculates the address of a supervisor-level interrupt file.
        pub const fn supervisor_interrupt_file_address(
            &self,
            hart_id: u32,
            group_id: u32,
        ) -> usize {
            self.supervisor_base
                + (group_id << self.group_bits) as usize
                + (hart_id << self.hart_offset_bits) as usize
        }

        /// Calculates the address of a guest interrupt file.
        ///
        /// Returns `None` if guest interrupt files are not supported.
        pub fn guest_interrupt_file_address(
            &self,
            hart_id: u32,
            group_id: u32,
            guest_index: u32,
        ) -> Option<usize> {
            self.guest_base.map(|base| {
                base + (group_id << self.group_bits) as usize
                    + (hart_id << self.guest_offset_bits) as usize
                    + (guest_index << self.guest_offset_bits) as usize
            })
        }
    }

    /// IMSIC capabilities and limits.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct Capabilities {
        /// Maximum number of interrupt identities supported.
        pub max_identities: u16,
        /// Number of guest interrupt files per hart.
        pub guest_files_per_hart: u32,
        /// Whether big-endian MSI support is available.
        pub big_endian_msi: bool,
        /// Whether priority thresholding is supported.
        pub priority_threshold: bool,
    }

    impl Capabilities {
        /// Creates capabilities for a standard IMSIC.
        pub const fn standard() -> Self {
            Self {
                max_identities: super::MAX_INTERRUPT_IDENTITY,
                guest_files_per_hart: crate::geilen::MAX_GUEST_FILES_PER_HART, // Maximum supported
                big_endian_msi: true,
                priority_threshold: true,
            }
        }

        /// Creates capabilities for a minimal IMSIC.
        pub const fn minimal() -> Self {
            Self {
                max_identities: 63, // Minimum supported
                guest_files_per_hart: 0,
                big_endian_msi: false,
                priority_threshold: false,
            }
        }
    }
}
