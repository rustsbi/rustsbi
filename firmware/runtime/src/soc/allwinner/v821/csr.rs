//! V821 PMA alias setup and Andes cache maintenance.

use crate::{
    Error, Result,
    memory::{PhysAddr, PhysAddrRange, SupervisorMemory},
};
use csr::{
    Read,
    vendor::andes::{self, McacheCtl, MmiscCtl},
};
pub use riscv_csr_spec::vendor::andes::CacheLineOperation as A27L2LineOperation;

/// An Andes machine-control status register exposed by the vendor SBI ABI.
#[derive(Clone, Copy)]
pub enum AndesStatusRegister {
    /// Machine cache-control status (`MCACHE_CTL`).
    MachineCacheControl,
    /// Machine miscellaneous-control status (`MMISC_CTL`).
    MachineMiscControl,
}

/// Offset of V821's physical mirror after Runtime configures its memory attributes.
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
/// Creation requires confirming the actual hardware and its execution environment.
#[derive(Clone, Copy)]
pub struct V821Csr {
    pma: PmaLayout,
}

// The empty RV64 enum makes V821Csr uninhabited on that target.
#[derive(Clone, Copy)]
enum PmaLayout {
    #[cfg(target_arch = "riscv32")]
    Pmacfg3,
}

impl super::AllwinnerV821Soc {
    /// Creates the custom-CSR interface for confirmed V821 hardware.
    ///
    /// Returns [`Error::NotSupported`] for an RV64 firmware image.
    ///
    /// # Safety
    ///
    /// 1. On RV32, verify that this description matches the actual V821 hardware.
    /// 2. Every hart using any copy of the handle must execute in M-mode with
    ///    Andes CCTL enabled and mappings that support cache writeback.
    pub const unsafe fn csr(self) -> Result<V821Csr> {
        #[cfg(target_arch = "riscv32")]
        {
            Ok(V821Csr {
                pma: PmaLayout::Pmacfg3,
            })
        }
        #[cfg(target_arch = "riscv64")]
        Err(Error::NotSupported)
    }
}

impl V821Csr {
    /// Physical cache-line size used by V821's A27L2 controller.
    pub const CACHE_LINE_SIZE: usize = 64;

    /// Marks the V821 physical mirror from 4 GiB up to 8 GiB as noncacheable.
    ///
    /// Returns [`Error::InvalidArgs`] if the hart is not an Andes implementation.
    ///
    /// # Safety
    ///
    /// The platform must implement the mirror and its writable PMA entry 15.
    /// Run before lower-mode or driver accesses use it. Own the containing
    /// configuration word through the read-modify-write and keep firmware
    /// accesses valid while the new attributes apply to the old address.
    pub unsafe fn initialize_noncacheable_alias(self) -> Result<NoncacheableAlias> {
        match self.pma {
            #[cfg(target_arch = "riscv32")]
            PmaLayout::Pmacfg3 => {
                use crate::instruction::{ALL, fence};
                use csr::{
                    Write,
                    standard::isa::privileged::machine::{Identifier, Mvendorid},
                    vendor::andes::{PmaAddress, PmaEntryConfig, Pmaaddr15, Pmacfg3},
                };

                const OFFSET: u64 = 0x1_0000_0000;
                if Mvendorid::read() != Identifier::from_bits(0x31e) {
                    return Err(Error::InvalidArgs);
                }
                let address = PmaAddress::napot(OFFSET, OFFSET)
                    .expect("V821's noncacheable alias has a representable NAPOT encoding");
                // SAFETY:
                // 1. The handle identifies the RV32 Andes CSRs on this V821.
                // 2. The caller owns the configuration word and keeps accesses
                //    valid until the address and attributes both describe the mirror.
                unsafe {
                    let mut cfg = Pmacfg3::read_unchecked().into_write();
                    cfg.set(Pmacfg3::ENTRY15, PmaEntryConfig::NONCACHEABLE_NAPOT);
                    Pmacfg3::write(cfg);
                    Pmaaddr15::write(address);
                }
                fence(ALL, ALL);
                Ok(NoncacheableAlias { offset: OFFSET })
            }
        }
    }

    /// Reads the selected Andes machine-control status register.
    pub fn read_status(self, register: AndesStatusRegister) -> usize {
        // SAFETY: Handle creation establishes the actual hardware and access mode.
        unsafe {
            match register {
                AndesStatusRegister::MachineCacheControl => McacheCtl::read_unchecked(),
                AndesStatusRegister::MachineMiscControl => MmiscCtl::read_unchecked(),
            }
        }
    }

    /// Applies cache maintenance only to a complete line in supervisor-owned RAM.
    ///
    /// Supervisor software coordinates DMA and decides when its dirty data may
    /// be discarded. The checked range excludes firmware-owned Rust allocations.
    pub fn maintain_a27l2_line(
        self,
        address: usize,
        operation: A27L2LineOperation,
        memory: &SupervisorMemory,
    ) -> Result<()> {
        let start = address & !(Self::CACHE_LINE_SIZE - 1);
        let end = start
            .checked_add(Self::CACHE_LINE_SIZE)
            .ok_or(Error::Overflow)?;
        memory.check_range(PhysAddrRange::new(
            PhysAddr::new(start),
            PhysAddr::new(end),
        )?)?;
        crate::irq::with_local_disabled(|_| {
            // SAFETY:
            // 1. The handle establishes CCTL access on this hart.
            // 2. Interrupt exclusion keeps the command and address paired.
            // 3. The range check covers the whole line, including both ends
            //    of a line containing an unaligned address.
            unsafe { andes::maintain_line(address, operation) };
        });
        Ok(())
    }

    /// Writes back and invalidates all A27L2 L1 data-cache lines.
    ///
    /// Dirty data is written back before invalidation. Supervisor software
    /// must coordinate concurrent DMA.
    pub fn write_back_and_invalidate_l1_all(self) {
        crate::irq::with_local_disabled(|_| {
            // SAFETY: The V821 capability establishes CCTL availability and IRQ
            // exclusion owns the port. The established PMA mappings support writeback,
            // which preserves dirty data before invalidation.
            unsafe { andes::write_back_and_invalidate_all() };
        });
    }
}
