//! Safe access to the V821 BSP's Andes custom CSRs.
//!
//! The `csr` crate defines the register encodings and hardware operations.
//! This module requires a V821 capability and selects the firmware's PMA alias.

use super::AllwinnerV821Soc;
use csr::vendor::andes::{
    PmaAddress, PmaEntryConfig,
    machine::{CacheControl, ENTRY15, MiscControl, PmaAddress15, PmaConfig3},
    user::CacheCommand,
};

pub use csr::vendor::andes::CacheLineOperation as A27L2LineOperation;

const NONCACHEABLE_ALIAS_OFFSET: u64 = 0x1_0000_0000;

/// An Andes machine-control status register exposed by the vendor SBI ABI.
#[derive(Clone, Copy)]
pub enum AndesStatusRegister {
    /// Machine cache-control status (`MCACHE_CTL`).
    MachineCacheControl,
    /// Machine miscellaneous-control status (`MMISC_CTL`).
    MachineMiscControl,
}

/// The noncacheable physical alias established by the V821 PMA setup.
///
/// Values are returned only after the reserved PMA entry has been programmed.
#[derive(Clone, Copy)]
#[must_use = "the alias offset is required to protect firmware memory in PMP"]
pub struct NoncacheableAlias {
    offset: u64,
}

impl NoncacheableAlias {
    /// Returns the offset from a cacheable physical address to its alias.
    pub const fn offset(self) -> u64 {
        self.offset
    }
}

/// Access to the Andes custom CSRs used by the V821 BSP.
///
/// Only a Devicetree-selected [`AllwinnerV821Soc`] can create this capability.
#[derive(Clone, Copy)]
pub struct V821Csr {
    _private: (),
}

impl AllwinnerV821Soc {
    /// Returns the custom-CSR interface authorized by this SoC description.
    pub const fn csr(self) -> V821Csr {
        V821Csr { _private: () }
    }
}

impl V821Csr {
    /// Configures the BSP-reserved PMA entry as the 4--8 GiB uncached alias.
    pub fn initialize_noncacheable_alias(self) -> NoncacheableAlias {
        let address = PmaAddress::napot(NONCACHEABLE_ALIAS_OFFSET, NONCACHEABLE_ALIAS_OFFSET)
            .expect("V821's noncacheable alias has a representable NAPOT encoding");
        // SAFETY: The V821 boot path owns PMA entry 15 in M-mode, preserves
        // other entries, and configures the alias before any driver uses it.
        unsafe {
            PmaConfig3::modify(|_, value| {
                value.set::<ENTRY15>(PmaEntryConfig::NONCACHEABLE_NAPOT);
            });
            PmaAddress15::write(address);
        }
        crate::instruction::fence();
        NoncacheableAlias {
            offset: NONCACHEABLE_ALIAS_OFFSET,
        }
    }

    /// Reads the selected Andes machine-control status register.
    pub fn read_status(self, register: AndesStatusRegister) -> usize {
        match register {
            AndesStatusRegister::MachineCacheControl => CacheControl::read().into(),
            AndesStatusRegister::MachineMiscControl => MiscControl::read().into(),
        }
    }

    /// Applies one cache-maintenance operation to a physical cache line.
    pub fn maintain_a27l2_line(self, address: usize, operation: A27L2LineOperation) {
        // SAFETY: The V821 capability identifies CCTL; the firmware driver
        // coordinates the command pair and validates the affected memory.
        unsafe { CacheCommand::maintain_line(address, operation) };
    }

    /// Writes back and invalidates all A27L2 L1 data-cache lines.
    pub fn write_back_and_invalidate_l1_all(self) {
        // SAFETY: The V821 firmware owns CCTL and permits whole-cache maintenance.
        unsafe { CacheCommand::write_back_and_invalidate_all() };
    }
}
