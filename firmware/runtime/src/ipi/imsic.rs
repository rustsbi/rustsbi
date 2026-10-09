//! Validated IMSIC configuration and hart-local file operations.
//!
//! An AIA/IMSIC platform may deliver the firmware IPI through a machine
//! external interrupt instead of the CLINT software-interrupt source. The
//! claim operation acknowledges the selected IMSIC IPI identity. This is the
//! only external-interrupt operation currently required by Runtime.

#![deny(unsafe_code)]

use crate::csr::{Mtopei, Readable};
use riscv_aia::Iid;
use riscv_aia::csrind::{eidelivery, eie, eip, eithreshold};

/// Validated machine IMSIC interrupt file used for firmware IPI transport.
#[derive(Clone, Copy, Debug)]
pub struct ImsicInterruptFile {
    num_ids: u16,
    ipi_iid: Iid,
}

/// An IMSIC operation could not be completed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ImsicError {
    /// The identity count or the firmware IPI identity is invalid.
    InvalidConfiguration,
    /// An architectural access faulted.
    Access(crate::trap::Error),
}

impl ImsicInterruptFile {
    /// Accepts identity counts 63, 127, ..., 2047 and an IPI within that range.
    pub const fn new(num_ids: u16, ipi_iid: Iid) -> Result<Self, ImsicError> {
        if num_ids < 63 || num_ids > 2047 || num_ids % 64 != 63 || ipi_iid.number() > num_ids {
            return Err(ImsicError::InvalidConfiguration);
        }
        Ok(Self { num_ids, ipi_iid })
    }

    /// Returns the firmware interrupt identity to write to an IMSIC MMIO page.
    pub const fn ipi_identity(self) -> Iid {
        self.ipi_iid
    }

    /// Clears this hart's file and enables delivery of the firmware IPI.
    #[allow(unsafe_code)]
    pub(super) fn initialize_current_hart(self) -> Result<(), ImsicError> {
        riscv::interrupt::machine::free(|| {
            // A non-claiming read verifies that this hart implements the file.
            Mtopei::read()?;
            // SAFETY:
            // 1. Runtime executes in M-mode; machine::free masks interrupts for indirect accesses.
            // 2. The non-claiming read above succeeded on this hart's machine file.
            // 3. new validated identity bounds; the loop selects valid RV32/RV64 indices.
            unsafe {
                eidelivery::machine::write(eidelivery::Eidelivery::ENABLED);
                eithreshold::machine::write(eithreshold::Eithreshold::from_bits(0));
                let num_regs = usize::from(self.num_ids).div_ceil(32);
                for index in 0..num_regs {
                    #[cfg(target_pointer_width = "64")]
                    if index % 2 == 1 {
                        continue;
                    }
                    eip::machine::write(index, eip::Eip::from_bits(0));
                    eie::machine::write(index, eie::Eie::from_bits(0));
                }
                let iid = usize::from(self.ipi_iid.number());
                #[cfg(target_pointer_width = "64")]
                let index = (iid / 64) * 2;
                #[cfg(target_pointer_width = "32")]
                let index = iid / 32;
                let enabled = eie::machine::read(index)
                    .set_enabled((iid % usize::BITS as usize) as u32, true);
                eie::machine::write(index, enabled);
            }
            Ok(())
        })
        .map_err(ImsicError::Access)
    }

    /// Atomically claims this hart's interrupt and reports whether it is the IPI.
    pub(super) fn acknowledge_current(self) -> Result<bool, ImsicError> {
        Mtopei::claim()
            .map(|value| value.identity() == self.ipi_iid.number())
            .map_err(ImsicError::Access)
    }
}
