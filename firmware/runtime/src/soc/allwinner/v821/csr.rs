//! V821 BSP Andes custom-CSR operations.
//!
//! V821 pairs Allwinner platform devices with Andes cores and an A27L2 cache.
//! Its BSP therefore uses Andes custom CSRs for PMA configuration, machine
//! status, and cache-line commands. These CSR numbers and command encodings are
//! isolated here; callers must present a V821 capability to use them.
//! The API remains visible on non-RISC-V targets for static analysis, but an
//! attempted hardware operation on such a target panics.
//!
//! # References
//!
//! - Vendor implementation: [V821 SPL cache support](https://github.com/sam-yangjj/tina-v821-v1.3-brandy/blob/34809037526678ccda1720cb4b4ec7ee32272c38/brandy-2.0/spl/arch/riscv/cpu/ads_rv32/mmu.c)
//!   — cache-control CSR numbers and command encodings.

use super::AllwinnerV821Soc;
use crate::csr::{Readable, Writable, native_registers};

native_registers! {
    ordered;
    read {
        MachineCacheControl: usize = 0x7ca;
        MachineMiscControl: usize = 0x7d0;
    }
    write {
        PmaConfig: usize = 0xbc3;
        PmaBoundary: usize = 0xbdf;
    }
    write_only {
        CacheAddress: usize = 0x80b;
        CacheCommand: usize = 0x80c;
    }
}

const NONCACHEABLE_ALIAS_OFFSET: u64 = 0x1_0000_0000;

/// An Andes machine-control status register exposed by the vendor SBI ABI.
#[derive(Clone, Copy)]
pub enum AndesStatusRegister {
    /// Machine cache-control status (`MCACHE_CTL`).
    MachineCacheControl,
    /// Machine miscellaneous-control status (`MMISC_CTL`).
    MachineMiscControl,
}

/// An Andes custom cache command accepted by the V821 BSP.
#[derive(Clone, Copy)]
pub enum A27L2LineOperation {
    /// Invalidate an L1 data-cache line.
    Invalidate,
    /// Write back a dirty L1 line.
    WriteBack,
    /// Write back and invalidate an L1 line.
    WriteBackAndInvalidate,
}

impl A27L2LineOperation {
    const fn encoding(self) -> usize {
        match self {
            Self::Invalidate => 0,
            Self::WriteBack => 1,
            Self::WriteBackAndInvalidate => 2,
        }
    }
}

/// The noncacheable physical alias established by the V821 PMA setup.
///
/// Values of this type are returned only after the custom PMA registers have
/// been programmed. Firmware policy can therefore use the offset without
/// duplicating the V821 register contract.
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

impl AllwinnerV821Soc {
    /// Configures the BSP-reserved PMA entry as the 4--8 GiB uncached alias.
    pub fn initialize_noncacheable_alias(self) -> NoncacheableAlias {
        const PMA_BOUNDARY_VALUE: usize = 0x5fff_ffff;
        const PMA_CONFIG_MASK: usize = 0xff00_0000;
        const PMA_CONFIG_VALUE: usize = 0x0f00_0000;
        let config = PmaConfig::read().expect("V821 PMA configuration read failed");
        PmaConfig::write((config & !PMA_CONFIG_MASK) | PMA_CONFIG_VALUE)
            .expect("V821 PMA configuration write failed");
        PmaBoundary::write(PMA_BOUNDARY_VALUE).expect("V821 PMA boundary write failed");
        riscv::asm::fence();
        NoncacheableAlias {
            offset: NONCACHEABLE_ALIAS_OFFSET,
        }
    }

    /// Reads the selected Andes machine-control status register.
    pub fn read_status(self, register: AndesStatusRegister) -> usize {
        match register {
            AndesStatusRegister::MachineCacheControl => MachineCacheControl::read(),
            AndesStatusRegister::MachineMiscControl => MachineMiscControl::read(),
        }
        .expect("V821 machine-control status read failed")
    }

    /// Applies one cache-maintenance operation to a physical cache line.
    pub fn maintain_a27l2_line(self, address: usize, operation: A27L2LineOperation) {
        riscv::asm::fence();
        CacheAddress::write(address).expect("V821 cache address write failed");
        CacheCommand::write(operation.encoding()).expect("V821 cache command write failed");
    }

    /// Writes back and invalidates all A27L2 L1 data-cache lines.
    pub fn write_back_and_invalidate_l1_all(self) {
        const WRITE_BACK_AND_INVALIDATE_ALL: usize = 6;
        riscv::asm::fence();
        CacheCommand::write(WRITE_BACK_AND_INVALIDATE_ALL)
            .expect("V821 cache command write failed");
    }
}
