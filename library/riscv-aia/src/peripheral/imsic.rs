//! Incoming MSI Controller (IMSIC) peripheral.

use volatile_register::WO;
use core::sync::atomic::{AtomicU32, Ordering};

/// Incoming MSI Controller (IMSIC) register block.
///
/// Each interrupt file in an IMSIC has one or two memory-mapped 32-bit
/// registers for receiving MSI writes. These memory-mapped registers are
/// located within a naturally aligned 4-KiB region (a page) of physical
/// address space that exists for the interrupt file.
#[repr(C)]
pub struct ImSic {
    /// 0x000 - Set interrupt-pending bit by number, little-endian.
    pub seteipnum_le: WO<u32>,
    /// 0x004 - Set interrupt-pending bit by number, big-endian.
    pub seteipnum_be: WO<u32>,
    // The rest of the 4-KiB page is reserved and read-only zeros.
}

impl ImSic {
    /// Returns the size of the IMSIC register block.
    pub const fn size() -> usize {
        0x1000 // 4-KiB page
    }
}

/// High-level IMSIC device state and behavior.
///
/// This struct models the runtime state behind an IMSIC interrupt file
/// and provides methods to handle MSI writes, pending/enable bits,
/// top-of-pending selection (topei), and simple priority storage.
pub struct ImSicDevice {
    /// External interrupt-pending registers (eip0..eip63)
    eip: [AtomicU32; 64],

    /// External interrupt-enable registers (eie0..eie63)
    eie: [AtomicU32; 64],

    /// Per-identity priority (index by identity). identity 0 unused.
    /// Stored as u16 to hold 11-bit priority values.
    priorities: [u16; (MAX_INTERRUPT_IDENTITY as usize) + 1],

    /// External interrupt delivery register (eidelivery)
    eidelivery: AtomicU32,

    /// External interrupt threshold register (eithreshold)
    eithreshold: AtomicU32,
    /// Optional delivery callback: called when an interrupt should be delivered.
    /// The callback receives the identity number.
    delivery_cb: Option<fn(u16)>,
    /// Whether this interrupt file is a guest file.
    is_guest: bool,
}

impl ImSicDevice {
    /// Creates a new `ImSicDevice` with default/reset values.
    pub fn new() -> Self {
        Self {
            eip: {
                // Initialize array of AtomicU32
                let mut uninit: [core::mem::MaybeUninit<AtomicU32>; 64] = unsafe { core::mem::MaybeUninit::uninit().assume_init() };
                let mut i = 0usize;
                while i < 64 {
                    uninit[i].write(AtomicU32::new(0));
                    i += 1;
                }
                unsafe { core::mem::transmute::<_, [AtomicU32; 64]>(uninit) }
            },
            eie: {
                let mut uninit: [core::mem::MaybeUninit<AtomicU32>; 64] = unsafe { core::mem::MaybeUninit::uninit().assume_init() };
                let mut i = 0usize;
                while i < 64 {
                    uninit[i].write(AtomicU32::new(0));
                    i += 1;
                }
                unsafe { core::mem::transmute::<_, [AtomicU32; 64]>(uninit) }
            },
            priorities: [0u16; (MAX_INTERRUPT_IDENTITY as usize) + 1],
            eidelivery: AtomicU32::new(Eidelivery::DISABLED.raw()),
            eithreshold: AtomicU32::new(0),
            delivery_cb: None,
            is_guest: false,
        }
    }

    /// Create a guest variant of the device.
    pub fn new_guest() -> Self {
        let mut s = Self::new();
        s.is_guest = true;
        s
    }

    /// Handle a write to `seteipnum_le` (little-endian MSI data).
    /// Decodes the identity and sets the corresponding pending bit.
    pub fn write_seteipnum_le(&self, data: u32) {
        if let Some(identity) = msi::decode_le(data) {
            self.set_pending(identity);
        }
    }

    /// Handle a write to `seteipnum_be` (big-endian MSI data).
    pub fn write_seteipnum_be(&self, data: u32) {
        if let Some(identity) = msi::decode_be(data) {
            self.set_pending(identity);
        }
    }

    /// Atomically set the pending bit for an identity.
    pub fn set_pending(&self, identity: u16) {
        if let Some((reg_idx, bit_pos)) = file_ops::identity_to_register(identity) {
            let mask = 1u32 << bit_pos;
            self.eip[reg_idx as usize].fetch_or(mask, Ordering::SeqCst);
            // If delivery conditions are met, invoke callback (if any).
            self.deliver_if_needed(identity);
        }
    }

    /// Atomically clear the pending bit for an identity.
    pub fn clear_pending(&self, identity: u16) {
        if let Some((reg_idx, bit_pos)) = file_ops::identity_to_register(identity) {
            let mask = !(1u32 << bit_pos);
            self.eip[reg_idx as usize].fetch_and(mask, Ordering::SeqCst);
        }
    }

    /// Set an enable bit for an identity.
    pub fn set_enable(&self, identity: u16, enabled: bool) {
        if let Some((reg_idx, bit_pos)) = file_ops::identity_to_register(identity) {
            let mask = 1u32 << bit_pos;
            if enabled {
                self.eie[reg_idx as usize].fetch_or(mask, Ordering::SeqCst);
            } else {
                self.eie[reg_idx as usize].fetch_and(!mask, Ordering::SeqCst);
            }
        }
    }

    /// Set or clear the delivery callback.
    pub fn set_delivery_callback(&mut self, cb: Option<fn(u16)>) {
        self.delivery_cb = cb;
    }

    /// Helper: whether an identity should trigger a delivery now.
    pub fn should_deliver(&self, identity: u16) -> bool {
        let ed = self.read_eidelivery();
        if ed.is_disabled() {
            return false;
        }

        if let Some((reg_idx, bit_pos)) = file_ops::identity_to_register(identity) {
            let pending = (self.read_eip(reg_idx as usize) & (1u32 << bit_pos)) != 0;
            let enabled = (self.read_eie(reg_idx as usize) & (1u32 << bit_pos)) != 0;
            if !pending || !enabled {
                return false;
            }
            let prio = self.priority(identity);
            let threshold = (self.read_eithreshold_raw() >> 24) as u16;
            return prio >= threshold;
        }
        false
    }

    /// Invoke delivery callback for identity if delivery conditions are met.
    pub fn deliver_if_needed(&self, identity: u16) {
        if self.should_deliver(identity) {
            if let Some(cb) = self.delivery_cb {
                cb(identity);
            }
        }
    }

    /// Read EIP register value (atomic snapshot).
    pub fn read_eip(&self, reg_idx: usize) -> u32 {
        self.eip[reg_idx].load(Ordering::SeqCst)
    }

    /// Read EIE register value (atomic snapshot).
    pub fn read_eie(&self, reg_idx: usize) -> u32 {
        self.eie[reg_idx].load(Ordering::SeqCst)
    }

    /// Set per-identity priority (0..=0x7FF used by Topei)
    pub fn set_priority(&mut self, identity: u16, prio: u16) {
        if file_ops::is_valid_identity(identity) {
            self.priorities[identity as usize] = prio & 0x7FF;
        }
    }

    /// Get per-identity priority
    pub fn priority(&self, identity: u16) -> u16 {
        if file_ops::is_valid_identity(identity) {
            self.priorities[identity as usize]
        } else {
            0
        }
    }

    /// Update `eidelivery` register.
    pub fn write_eidelivery(&self, val: Eidelivery) {
        self.eidelivery.store(val.raw(), Ordering::SeqCst);
    }

    /// Read `eidelivery` register.
    pub fn read_eidelivery(&self) -> Eidelivery {
        Eidelivery::from_raw(self.eidelivery.load(Ordering::SeqCst))
    }

    /// Update `eithreshold` register raw value.
    pub fn write_eithreshold_raw(&self, raw: u32) {
        self.eithreshold.store(raw, Ordering::SeqCst);
    }

    /// Read `eithreshold` raw value.
    pub fn read_eithreshold_raw(&self) -> u32 {
        self.eithreshold.load(Ordering::SeqCst)
    }

    /// Select the top pending & enabled interrupt following priority rules and
    /// claim it (clear its pending bit). Returns `Topei::NONE` if none.
    pub fn claim_topei(&self) -> Topei {
        // Select interrupt with highest priority value, tie-break by lower identity.
        let threshold = (self.read_eithreshold_raw() >> 24) as u16; // priority field is 11-bit

        let mut best_identity: Option<u16> = None;
        let mut best_prio: u16 = 0;

        for identity in 1..=MAX_INTERRUPT_IDENTITY {
            let (reg_idx, bit_pos) = match file_ops::identity_to_register(identity) {
                Some(p) => p,
                None => continue,
            };
            let pending = (self.read_eip(reg_idx as usize) & (1u32 << bit_pos)) != 0;
            let enabled = (self.read_eie(reg_idx as usize) & (1u32 << bit_pos)) != 0;
            if pending && enabled {
                let prio = self.priority(identity);
                if prio >= threshold {
                    match best_identity {
                        None => {
                            best_identity = Some(identity);
                            best_prio = prio;
                        }
                        Some(curr) => {
                            if prio > best_prio || (prio == best_prio && identity < curr) {
                                best_identity = Some(identity);
                                best_prio = prio;
                            }
                        }
                    }
                }
            }
        }

        if let Some(id) = best_identity {
            self.clear_pending(id);
            let raw = ((id as u32) << 16) | (u32::from(best_prio) & 0x7FF);
            Topei::from_raw(raw)
        } else {
            Topei::NONE
        }
    }
}


/// External interrupt delivery enable register (`eidelivery`).
///
/// This register controls whether interrupts from this interrupt file are
/// delivered from the IMSIC to the attached hart.
///
/// Note: Guest interrupt files do not support value 0x40000000 for `eidelivery`.
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
        (self.0 & (1 << index)) != 0
    }

    /// Sets the pending status for the interrupt with the given index (0-31).
    pub const fn set_pending(mut self, index: u32, pending: bool) -> Self {
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
        (self.0 & (1 << index)) != 0
    }

    /// Sets the enable status for the interrupt with the given index (0-31).
    pub const fn set_enabled(mut self, index: u32, enabled: bool) -> Self {
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
/// Note: Interrupt identity 0 is never valid.
/// Lower-numbered interrupt identities have higher priority than higher-numbered ones.
pub const MIN_INTERRUPT_IDENTITY: u16 = 1;

/// MSI (Message-Signaled Interrupt) encoding utilities.
pub mod msi {
    use super::{MAX_INTERRUPT_IDENTITY, MIN_INTERRUPT_IDENTITY};

    /// Encodes an interrupt identity into MSI data.
    ///
    /// Returns the 32-bit MSI data value for the given interrupt identity.
    /// The MSI data is simply the interrupt identity number in little-endian byte order.
    #[inline]
    pub const fn encode_le(identity: u16) -> u32 {
        identity as u32
    }

    /// Encodes an interrupt identity into big-endian MSI data.
    ///
    /// Returns the 32-bit MSI data value for the given interrupt identity
    /// in big-endian byte order.
    #[inline]
    pub const fn encode_be(identity: u16) -> u32 {
        (identity as u32) << 16
    }

    /// Decodes MSI data into an interrupt identity (little-endian).
    ///
    /// Returns `Some(identity)` if the decoded value is a valid interrupt identity,
    /// or `None` if the value is invalid (0 or > MAX_INTERRUPT_IDENTITY).
    #[inline]
    pub const fn decode_le(data: u32) -> Option<u16> {
        let identity = data as u16;
        if identity >= MIN_INTERRUPT_IDENTITY && identity <= MAX_INTERRUPT_IDENTITY {
            Some(identity)
        } else {
            None
        }
    }

    /// Decodes MSI data into an interrupt identity (big-endian).
    ///
    /// Returns `Some(identity)` if the decoded value is a valid interrupt identity,
    /// or `None` if the value is invalid (0 or > MAX_INTERRUPT_IDENTITY).
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

    /// Calculates the register index and bit position for an interrupt identity.
    ///
    /// Returns `(register_index, bit_position)` where:
    /// - `register_index` is the index into the eip/eie array (0-63)
    /// - `bit_position` is the bit position within that register (0-31)
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
            let mut update_count = 0;

            for &identity in identities {
                if let Some((reg_idx, bit_pos)) = identity_to_register(identity) {
                    if reg_idx < 64 {
                        // Check if this register already has an update
                        let mut found = false;
                        for i in 0..update_count {
                            if updates[i].0 == reg_idx {
                                updates[i].1 = updates[i].1.set_pending(bit_pos, true);
                                found = true;
                                break;
                            }
                        }
                        if !found {
                            updates[update_count].0 = reg_idx;
                            updates[update_count].1 = Eip::from_raw(0).set_pending(bit_pos, true);
                            update_count += 1;
                        }
                    }
                }
            }

            // Mark unused entries as invalid (register index will be > 63)
            for i in update_count..64 {
                updates[i].0 = u32::MAX;
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
        /// Creates a default address layout.
        ///
        /// This follows the recommended layout from the AIA specification.
        pub const fn default() -> Self {
            Self {
                machine_base: 0x2800_0000,
                supervisor_base: 0x2800_4000,
                guest_base: Some(0x2800_8000),
                hart_index_bits: 12,   // j = 12
                group_bits: 24,        // E = 24
                hart_offset_bits: 12,  // C = 12
                guest_offset_bits: 12, // D = 12
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
                guest_files_per_hart: 63, // GEILEN max
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

/// Indirect CSR access utilities.
///
/// This module provides utilities for accessing IMSIC registers indirectly
/// through the `miselect` and `mireg` CSRs, as defined in the AIA specification.
/// Indirect access allows software to read and write IMSIC registers that are
/// not directly accessible as CSRs.
pub mod indirect_access {
    use super::{select, file_ops};
    // use crate::register::{miselect::Miselect, mireg::Mireg};

    /// Errors that can occur during indirect register access.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum IndirectAccessError {
        /// Invalid select value for the current privilege level.
        InvalidSelectValue,
        /// Attempted to access a read-only register for writing.
        ReadOnlyRegister,
        /// Attempted to access a write-only register for reading.
        WriteOnlyRegister,
    }

    /// Trait for types that can be accessed indirectly.
    ///
    /// This trait is implemented by IMSIC register types that support
    /// indirect access through `miselect` and `mireg`.
    pub trait IndirectAccessible {
        /// Returns the select value for this register.
        fn select_value(&self) -> u32;

        /// Converts the register value to a raw u32 for indirect access.
        fn to_raw(&self) -> u32;

        /// Creates a register instance from a raw u32 value.
        fn from_raw(raw: u32) -> Self;
    }

    impl IndirectAccessible for super::Eidelivery {
        fn select_value(&self) -> u32 {
            select::EIDELIVERY
        }

        fn to_raw(&self) -> u32 {
            self.raw()
        }

        fn from_raw(raw: u32) -> Self {
            Self::from_raw(raw)
        }
    }

    impl IndirectAccessible for super::Eithreshold {
        fn select_value(&self) -> u32 {
            select::EITHRESHOLD
        }

        fn to_raw(&self) -> u32 {
            self.raw()
        }

        fn from_raw(raw: u32) -> Self {
            Self::from_raw(raw)
        }
    }

    impl IndirectAccessible for super::Topei {
        fn select_value(&self) -> u32 {
            select::EIDELIVERY + 0x40 // topei is at offset 0x40 from eidelivery
        }

        fn to_raw(&self) -> u32 {
            self.raw()
        }

        fn from_raw(raw: u32) -> Self {
            Self::from_raw(raw)
        }
    }

    /// Indirect register accessor.
    ///
    /// This struct provides methods to perform indirect register access
    /// to IMSIC registers. It abstracts the low-level `miselect` and `mireg`
    /// operations.
    pub struct IndirectAccessor;

    impl IndirectAccessor {
        /// Read a raw u32 value from IMSIC indirectly using a `select` value.
        ///
        /// `device` is the runtime IMSIC state. `select` is the value that would
        /// be written to `miselect` CSR to select the target register.
        pub unsafe fn read_raw(device: &super::ImSicDevice, select: u32) -> Result<u32, IndirectAccessError> {
            Self::validate_select_value(select)?;

            if select == select::EIDELIVERY {
                return Ok(device.read_eidelivery().raw());
            }

            if select == select::EITHRESHOLD {
                return Ok(device.read_eithreshold_raw());
            }

            if select == select::EIDELIVERY + 0x40 {
                // topei read returns and claims top pending
                return Ok(device.claim_topei().raw());
            }

            if select >= select::EIP_BASE && select < select::EIP_BASE + 64 {
                let idx = (select - select::EIP_BASE) as usize;
                return Ok(device.read_eip(idx));
            }

            if select >= select::EIE_BASE && select < select::EIE_BASE + 64 {
                let idx = (select - select::EIE_BASE) as usize;
                return Ok(device.read_eie(idx));
            }

            Err(IndirectAccessError::InvalidSelectValue)
        }

        /// Write a raw u32 value to IMSIC indirectly using a `select` value.
        ///
        /// For registers that are read-only (e.g. topei), returns
        /// `IndirectAccessError::ReadOnlyRegister`.
        pub unsafe fn write_raw(device: &super::ImSicDevice, select: u32, value: u32) -> Result<(), IndirectAccessError> {
            Self::validate_select_value(select)?;

            if select == select::EIDELIVERY {
                // Guest interrupt files must not enable PLIC/APLIC delivery (0x4000_0000)
                if device.is_guest && value == super::Eidelivery::PLIC_APLIC_ENABLED.raw() {
                    return Err(IndirectAccessError::InvalidSelectValue);
                }
                device.write_eidelivery(super::Eidelivery::from_raw(value));
                return Ok(());
            }

            if select == select::EITHRESHOLD {
                // Guest interrupt files should not be allowed to modify threshold
                if device.is_guest {
                    return Err(IndirectAccessError::InvalidSelectValue);
                }
                device.write_eithreshold_raw(value);
                return Ok(());
            }

            if select == select::EIDELIVERY + 0x40 {
                // topei is read-only
                return Err(IndirectAccessError::ReadOnlyRegister);
            }

            if select >= select::EIP_BASE && select < select::EIP_BASE + 64 {
                // Guest files are not allowed to directly write EIP registers via
                // indirect CSR access; MSI memory-mapped writes still work.
                if device.is_guest {
                    return Err(IndirectAccessError::InvalidSelectValue);
                }
                let idx = (select - select::EIP_BASE) as usize;
                // Writing raw value sets pending bits directly. Call delivery for
                // any newly set bits.
                let prev = device.eip[idx].load(core::sync::atomic::Ordering::SeqCst);
                let new = value;
                let delta = new & !prev;
                device.eip[idx].store(new, core::sync::atomic::Ordering::SeqCst);
                if delta != 0 {
                    let mut bit = 0u32;
                    while bit < 32 {
                        if (delta & (1u32 << bit)) != 0 {
                            if let Some(identity) = file_ops::register_to_identity(idx as u32, bit) {
                                device.deliver_if_needed(identity);
                            }
                        }
                        bit += 1;
                    }
                }
                return Ok(());
            }

            if select >= select::EIE_BASE && select < select::EIE_BASE + 64 {
                // Guest files are not allowed to directly modify EIE via indirect
                // CSR access.
                if device.is_guest {
                    return Err(IndirectAccessError::InvalidSelectValue);
                }
                let idx = (select - select::EIE_BASE) as usize;
                device.eie[idx].store(value, core::sync::atomic::Ordering::SeqCst);
                return Ok(());
            }

            Err(IndirectAccessError::InvalidSelectValue)
        }

        /// Validates a select value for indirect access.
        pub fn validate_select_value(select: u32) -> Result<(), IndirectAccessError> {
            if select == select::EIDELIVERY
                || select == select::EITHRESHOLD
                || (select >= select::EIP_BASE && select <= select::EIP_BASE + 63)
                || (select >= select::EIE_BASE && select <= select::EIE_BASE + 63)
                || select == select::EIDELIVERY + 0x40
            {
                Ok(())
            } else {
                Err(IndirectAccessError::InvalidSelectValue)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::mem::size_of;
    use memoffset::offset_of;

    #[test]
    fn struct_imsic_offset() {
        assert_eq!(size_of::<ImSic>(), 0x8);

        assert_eq!(offset_of!(ImSic, seteipnum_le), 0x0);
        assert_eq!(offset_of!(ImSic, seteipnum_be), 0x4);
    }

    #[test]
    fn eidelivery_values() {
        assert!(Eidelivery::DISABLED.is_disabled());
        assert!(!Eidelivery::DISABLED.is_enabled());
        assert!(!Eidelivery::DISABLED.is_plic_aplic_enabled());

        assert!(!Eidelivery::ENABLED.is_disabled());
        assert!(Eidelivery::ENABLED.is_enabled());
        assert!(!Eidelivery::ENABLED.is_plic_aplic_enabled());

        assert!(!Eidelivery::PLIC_APLIC_ENABLED.is_disabled());
        assert!(!Eidelivery::PLIC_APLIC_ENABLED.is_enabled());
        assert!(Eidelivery::PLIC_APLIC_ENABLED.is_plic_aplic_enabled());
    }

    #[test]
    fn eip_operations() {
        let eip = Eip::from_raw(0b1010);
        assert!(eip.is_pending(1));
        assert!(!eip.is_pending(0));
        assert!(eip.is_pending(3));
        assert!(!eip.is_pending(2));

        let eip_modified = eip.set_pending(0, true).set_pending(1, false);
        assert!(eip_modified.is_pending(0));
        assert!(!eip_modified.is_pending(1));
        assert!(eip_modified.is_pending(3));
    }

    #[test]
    fn eie_operations() {
        let eie = Eie::from_raw(0b1100);
        assert!(eie.is_enabled(2));
        assert!(eie.is_enabled(3));
        assert!(!eie.is_enabled(0));
        assert!(!eie.is_enabled(1));

        let eie_modified = eie.set_enabled(0, true).set_enabled(2, false);
        assert!(eie_modified.is_enabled(0));
        assert!(!eie_modified.is_enabled(2));
        assert!(eie_modified.is_enabled(3));
    }

    #[test]
    fn topei_values() {
        assert!(!Topei::NONE.is_pending());
        assert_eq!(Topei::NONE.interrupt_identity(), 0);
        assert_eq!(Topei::NONE.priority(), 0);

        let topei = Topei::from_raw(0x0001_0005); // identity 1, priority 5
        assert!(topei.is_pending());
        assert_eq!(topei.interrupt_identity(), 1);
        assert_eq!(topei.priority(), 5);
    }

    #[test]
    fn msi_encoding() {
        // Test little-endian encoding/decoding
        let identity = 42u16;
        let encoded = msi::encode_le(identity);
        assert_eq!(encoded, 42);
        assert_eq!(msi::decode_le(encoded), Some(identity));

        // Test big-endian encoding/decoding
        let encoded_be = msi::encode_be(identity);
        assert_eq!(msi::decode_be(encoded_be), Some(identity));

        // Test invalid identities
        assert_eq!(msi::decode_le(0), None);
        assert_eq!(msi::decode_le(3000), None);
    }

    #[test]
    fn msi_triggers_delivery_callback() {
        use core::sync::atomic::{AtomicU32, Ordering};

        static DELIVERED: AtomicU32 = AtomicU32::new(0);

        fn cb(id: u16) {
            // Write to the static to indicate delivery.
            DELIVERED.store(id as u32, Ordering::SeqCst);
        }

        let mut dev = ImSicDevice::new();

        // Set up: identity 1, priority 5, enabled, delivery enabled
        dev.set_priority(1, 5);
        dev.set_enable(1, true);
        dev.write_eidelivery(Eidelivery::ENABLED);

        // Install callback (requires mutable borrow)
        dev.set_delivery_callback(Some(cb));

        // Ensure nothing delivered yet
        assert_eq!(DELIVERED.load(Ordering::SeqCst), 0);

        // Write MSI little-endian for identity 1
        dev.write_seteipnum_le(msi::encode_le(1));

        // Callback should have been invoked synchronously
        assert_eq!(DELIVERED.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn topei_claim_selects_highest_priority_and_clears_pending() {
        let mut dev = ImSicDevice::new();

        // identity 1: priority 3
        dev.set_priority(1, 3);
        dev.set_enable(1, true);

        // identity 2: priority 5 (higher)
        dev.set_priority(2, 5);
        dev.set_enable(2, true);

        // Set both pending via MSI writes
        dev.write_seteipnum_le(msi::encode_le(1));
        dev.write_seteipnum_le(msi::encode_le(2));

        // Claim top pending
        let top = dev.claim_topei();
        assert!(top.is_pending());
        // Should return identity 2 because it has higher priority
        assert_eq!(top.interrupt_identity(), 2);
        assert_eq!(top.priority(), 5);

        // Pending for identity 2 should be cleared
        if let Some((reg_idx, bit_pos)) = file_ops::identity_to_register(2) {
            let eip = dev.read_eip(reg_idx as usize);
            assert_eq!((eip & (1u32 << bit_pos)) != 0, false);
        } else {
            panic!("invalid identity")
        }
    }

    #[test]
    fn identity_to_register_conversion() {
        // Test identity 1 (first bit of first register)
        assert_eq!(file_ops::identity_to_register(1), Some((0, 0)));

        // Test identity 32 (last bit of first register)
        assert_eq!(file_ops::identity_to_register(32), Some((0, 31)));

        // Test identity 33 (first bit of second register)
        assert_eq!(file_ops::identity_to_register(33), Some((1, 0)));

        // Test identity 2047 (last valid identity)
        assert_eq!(file_ops::identity_to_register(2047), Some((63, 30))); // (2047-1)/32 = 63, (2047-1)%32 = 30
    }

    #[test]
    fn register_to_identity_conversion() {
        assert_eq!(file_ops::register_to_identity(0, 0), Some(1));
        assert_eq!(file_ops::register_to_identity(0, 31), Some(32));
        assert_eq!(file_ops::register_to_identity(1, 0), Some(33));
        assert_eq!(file_ops::register_to_identity(63, 30), Some(2047));

        // Invalid cases
        assert_eq!(file_ops::register_to_identity(64, 0), None);
        assert_eq!(file_ops::register_to_identity(0, 32), None);
    }

    #[test]
    fn select_values() {
        assert_eq!(file_ops::eip_select(0), select::EIP_BASE);
        assert_eq!(file_ops::eie_select(0), select::EIE_BASE);
        assert_eq!(file_ops::eip_select(1), select::EIP_BASE + 1);
        assert_eq!(file_ops::eie_select(1), select::EIE_BASE + 1);
    }

    #[test]
    fn identity_iterator() {
        let mut iter = file_ops::IdentityIterator::new();
        assert_eq!(iter.next(), Some(1));
        assert_eq!(iter.next(), Some(2));
        // Skip to near the end
        for _ in 3..2047 {
            iter.next();
        }
        assert_eq!(iter.next(), Some(2047));
        assert_eq!(iter.next(), None);
    }

    #[test]
    fn bulk_operations() {
        let identities = [1, 32, 33, 64];
        let pending_updates = file_ops::bulk::set_pending_batch(&identities);

        // Count valid updates (register index != u32::MAX)
        let mut valid_count = 0;
        for (idx, _) in pending_updates.iter() {
            if *idx != u32::MAX {
                valid_count += 1;
            }
        }

        // Should have updates for registers 0 and 1 (identity 64 also goes to register 1)
        assert_eq!(valid_count, 2);

        // Check register 0 (identities 1, 32)
        let reg0_update = pending_updates.iter().find(|(idx, _)| *idx == 0).unwrap();
        assert!(reg0_update.1.is_pending(0)); // identity 1
        assert!(reg0_update.1.is_pending(31)); // identity 32

        // Check register 1 (identities 33, 64)
        let reg1_update = pending_updates.iter().find(|(idx, _)| *idx == 1).unwrap();
        assert!(reg1_update.1.is_pending(0)); // identity 33
        assert!(reg1_update.1.is_pending(31)); // identity 64
    }

    #[test]
    fn address_layout() {
        let layout = system::AddressLayout::default();

        // Test machine interrupt file address
        let addr = layout.machine_interrupt_file_address(1, 0);
        assert_eq!(addr, 0x2800_0000 + (1 << 12));

        // Test supervisor interrupt file address
        let addr = layout.supervisor_interrupt_file_address(2, 0);
        assert_eq!(addr, 0x2800_4000 + (2 << 12));

        // Test guest interrupt file address
        let addr = layout.guest_interrupt_file_address(1, 0, 1);
        assert_eq!(addr, Some(0x2800_8000 + (1 << 12) + (1 << 12)));
    }

    #[test]
    fn capabilities() {
        let standard = system::Capabilities::standard();
        assert_eq!(standard.max_identities, MAX_INTERRUPT_IDENTITY);
        assert_eq!(standard.guest_files_per_hart, 63);
        assert!(standard.big_endian_msi);
        assert!(standard.priority_threshold);

        let minimal = system::Capabilities::minimal();
        assert_eq!(minimal.max_identities, 63);
        assert_eq!(minimal.guest_files_per_hart, 0);
        assert!(!minimal.big_endian_msi);
        assert!(!minimal.priority_threshold);
    }

    #[test]
    fn guest_eidelivery_rejects_plic_aplic() {
        let dev = ImSicDevice::new_guest();
        // Attempt to write PLIC/APLIC enabled value via indirect access
        let res = unsafe { crate::peripheral::imsic::indirect_access::IndirectAccessor::write_raw(&dev, select::EIDELIVERY, Eidelivery::PLIC_APLIC_ENABLED.raw()) };
        assert_eq!(res, Err(crate::peripheral::imsic::indirect_access::IndirectAccessError::InvalidSelectValue));
    }

    #[test]
    fn guest_write_eithreshold_rejected() {
        let dev = ImSicDevice::new_guest();
        let res = unsafe { crate::peripheral::imsic::indirect_access::IndirectAccessor::write_raw(&dev, select::EITHRESHOLD, 0x12345678) };
        assert_eq!(res, Err(crate::peripheral::imsic::indirect_access::IndirectAccessError::InvalidSelectValue));
    }

    #[test]
    fn guest_write_eip_rejected() {
        let dev = ImSicDevice::new_guest();
        // Attempt to write eip0 via indirect access
        let sel = select::EIP_BASE;
        let res = unsafe { crate::peripheral::imsic::indirect_access::IndirectAccessor::write_raw(&dev, sel, 0xFFFF_FFFF) };
        assert_eq!(res, Err(crate::peripheral::imsic::indirect_access::IndirectAccessError::InvalidSelectValue));
    }

    #[test]
    fn guest_write_eie_rejected() {
        let dev = ImSicDevice::new_guest();
        // Attempt to write eie0 via indirect access
        let sel = select::EIE_BASE;
        let res = unsafe { crate::peripheral::imsic::indirect_access::IndirectAccessor::write_raw(&dev, sel, 0xFFFF_FFFF) };
        assert_eq!(res, Err(crate::peripheral::imsic::indirect_access::IndirectAccessError::InvalidSelectValue));
    }

    #[test]
    fn indirect_topei_claim_via_mireg() {
        // End-to-end: MSI write sets pending, then a mireg read of topei returns and claims it.
        let mut dev = ImSicDevice::new();

        // identity 10: priority 7
        dev.set_priority(10, 7);
        dev.set_enable(10, true);
        // ensure delivery isn't necessary for claim test, threshold low
        dev.write_eithreshold_raw(0);

        // write MSI little-endian for identity 10
        dev.write_seteipnum_le(msi::encode_le(10));

        // Read topei via indirect accessor (select == EIDELIVERY + 0x40)
        let raw = unsafe { crate::peripheral::imsic::indirect_access::IndirectAccessor::read_raw(&dev, select::EIDELIVERY + 0x40) }.expect("topei read");
        let topei = Topei::from_raw(raw);
        assert!(topei.is_pending());
        assert_eq!(topei.interrupt_identity(), 10);

        // Ensure pending for identity 10 is now cleared
        if let Some((reg_idx, bit_pos)) = file_ops::identity_to_register(10) {
            let eip = dev.read_eip(reg_idx as usize);
            assert_eq!((eip & (1u32 << bit_pos)) != 0, false);
        } else {
            panic!("invalid identity")
        }
    }
}
