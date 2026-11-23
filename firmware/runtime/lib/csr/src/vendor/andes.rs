//! Andes CSRs used by the V821 BSP.
//!
//! Register layouts and cache commands follow the [AndesCore AX45MP manual],
//! sections 16.12.6--16.12.7, 16.12.13--16.12.14, and 16.17. The RV32 PMA
//! register packing is also documented in the [HPM6E00 manual], section 3.2.119.
//! The [V821 BSP] confirms the cache CSR numbers and command subset used here.
//!
//! [AndesCore AX45MP manual]: https://www.andestech.com/wp-content/uploads/AX45MP-1C-Rev.-5.0.0-Datasheet.pdf
//! [HPM6E00 manual]: https://www.hpmicro.com/Public/Uploads/uploadfile/files/20250109/HPM6E00UMV06.pdf
//! [V821 BSP]: https://github.com/sam-yangjj/tina-v821-v1.3-brandy/blob/34809037526678ccda1720cb4b4ec7ee32272c38/brandy-2.0/spl/arch/riscv/cpu/ads_rv32/mmu.c

use crate::{macros::csr, value::Value};

/// One PMA entry's attribute byte, including unknown read encodings.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PmaEntryConfig(u8);

impl PmaEntryConfig {
    /// Bufferable, non-cacheable NAPOT memory with AMOs permitted.
    pub const NONCACHEABLE_NAPOT: Self = Self(0x0f);
}

impl Value for PmaEntryConfig {
    const BITS: u32 = 8;

    fn from_bits(bits: usize) -> Self {
        Self(bits as u8)
    }

    fn bits(self) -> usize {
        self.0 as usize
    }
}

csr! {
    /// A PMA address-register operand (physical address shifted by two bits).
    pub struct PmaAddress(usize);
}

impl PmaAddress {
    /// Encodes an aligned NAPOT range of at least 4 KiB, if it fits.
    ///
    /// Range arithmetic must fit `u64`, and the encoded operand must fit XLEN.
    /// The caller must check the PMA granularity implemented by its hart.
    pub const fn napot(base: u64, size: u64) -> Option<Self> {
        if size < 4096
            || !size.is_power_of_two()
            || base & (size - 1) != 0
            || base.checked_add(size).is_none()
        {
            return None;
        }
        let encoded = (base >> 2) | (size / 8 - 1);
        if encoded > usize::MAX as u64 {
            return None;
        }
        Some(Self(encoded as usize))
    }
}

/// Andes machine-level registers used by V821.
pub mod machine {
    use super::{PmaAddress, PmaEntryConfig};
    use crate::macros::csr;

    csr! {
        /// Machine cache control (`mcache_ctl`).
        CacheControl { 0x7ca, r }
        /// Machine miscellaneous control (`mmisc_ctl`).
        MiscControl { 0x7d0, r }
        /// RV32 PMA configuration entries 12 through 15 (`pmacfg3`).
        PmaConfig3 { 0xbc3, r,
            /// # Safety
            ///
            /// Execute in M-mode on an RV32 Andes hart implementing PMA.
            /// Own the changed entries, preserve the other entries, and
            /// synchronize memory accesses and cache state across the change.
            unsafe w
        }
            ENTRY15[24..=31] { rw: PmaEntryConfig }
        /// PMA entry 15's encoded address (`pmaaddr15`).
        PmaAddress15 { 0xbdf, r: PmaAddress,
            /// # Safety
            ///
            /// Own PMA entry 15 and supply a range supported by this hart.
            /// Coordinate its attributes, cache state, and affected accesses.
            unsafe w: PmaAddress
        }
    }
}

/// Address-based L1 data-cache commands (Andes manual, section 16.12.14).
#[repr(usize)]
#[derive(Clone, Copy)]
pub enum CacheLineOperation {
    /// Invalidates the selected line.
    Invalidate = 0,
    /// Writes back the selected line.
    WriteBack = 1,
    /// Writes back and invalidates the selected line.
    WriteBackAndInvalidate = 2,
}

/// Andes supervisor/user cache-control registers, also accessible in M-mode.
pub mod user {
    use super::CacheLineOperation;
    use crate::macros::csr;

    csr! {
        /// Cache-control operand address (`ucctlbeginaddr`).
        CacheBeginAddress { 0x80b }
        /// Cache-control command port (`ucctlcommand`).
        CacheCommand { 0x80c }
    }

    impl CacheCommand {
        /// Orders memory, selects an address, and issues one cache-line command.
        ///
        /// # Safety
        ///
        /// Establish access to Andes CCTL and exclusive use of this hart's
        /// address/command pair. Coordinate affected memory accesses and ensure
        /// invalidation cannot discard live dirty data.
        #[inline]
        pub unsafe fn maintain_line(address: usize, operation: CacheLineOperation) {
            #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
            // SAFETY: The caller owns the CCTL pair and the enum limits commands.
            unsafe {
                core::arch::asm!(
                    "fence",
                    "csrw {address_csr}, {address}",
                    "csrw {command_csr}, {command}",
                    address_csr = const CacheBeginAddress::NUMBER,
                    command_csr = const Self::NUMBER,
                    address = in(reg) address,
                    command = in(reg) operation as usize,
                    options(nostack),
                );
            }
            #[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
            {
                let _ = (address, operation);
                unimplemented!("Andes CCTL requires a RISC-V target");
            }
        }

        /// Orders memory and writes back and invalidates every L1 data-cache line.
        ///
        /// # Safety
        ///
        /// Establish access to Andes CCTL, coordinate this hart's command port
        /// and affected memory accesses, and permit whole-cache maintenance.
        #[inline]
        pub unsafe fn write_back_and_invalidate_all() {
            #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
            // SAFETY: The caller coordinates the command port and cache state.
            unsafe {
                core::arch::asm!(
                    "fence", "csrw {csr}, {command}",
                    csr = const Self::NUMBER,
                    command = in(reg) 6usize,
                    options(nostack),
                );
            }
            #[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
            unimplemented!("Andes CCTL requires a RISC-V target");
        }
    }
}
