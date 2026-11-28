//! RISC-V control and status register (CSR) access for RustSBI firmware.
//!
//! [`standard`] and [`vendor`] re-export the definitions from [`riscv_csr_spec`].
//! [`Read`], [`Write`], and [`Claim`] operate on the current hart.
//! A standard CSR with [`StandardRead`] permits a safe read.
//! Vendor reads, writes, claims, and [recoverable accesses](recovery) have caller obligations.

#![no_std]
#![deny(missing_docs)]

#[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
compile_error!("rustsbi-csr requires RISC-V; use riscv-csr-spec for portable definitions");

pub use riscv_csr_spec::{Clear, Csr, Field, Mask, ReadMode, Set, StandardRead, Width, WriteMode};
use riscv_csr_spec::{ClaimSpec, ReadSpec, WriteSpec};

mod instruction;
pub mod recovery;
pub mod standard;
pub mod vendor;

/// CSR access that reads a value without requesting a write.
pub trait Read: ReadSpec {
    /// Reads and decodes a standard CSR on the current hart.
    ///
    /// Unavailable or forbidden access traps.
    /// On RV32, [counter reads](https://docs.riscv.org/reference/isa/v20260120/unpriv/counters.html)
    /// account for a carry from the low word.
    /// A concurrent write to a configuration register or timer comparator
    /// can yield a snapshot containing words from different values.
    #[inline(always)]
    fn read() -> Self::Value
    where
        Self: StandardRead,
    {
        // SAFETY: StandardRead admits only standard CSRs with side-effect-free reads.
        unsafe { Self::read_unchecked() }
    }

    /// Reads and decodes a CSR, including a vendor-specific register.
    ///
    /// Unavailable or forbidden access traps.
    ///
    /// # Safety
    ///
    /// If the CSR exists, establish that this number has the declared meaning
    /// on the current hart, including its model and revision for vendor CSRs.
    /// Coordinate any read effects with other users of the device and its memory.
    #[inline(always)]
    unsafe fn read_unchecked() -> Self::Value {
        // SAFETY: The caller establishes applicability and owns any read effects.
        unsafe { instruction::Direct::read::<Self>() }
            .expect("unprotected CSR reads cannot return a recovery error")
    }

    /// Reads a CSR and returns [`AccessError::IllegalInstruction`] if the instruction traps as illegal.
    ///
    /// Uses the same decoding and RV32 read protocol as [`Self::read_unchecked`].
    ///
    /// # Safety
    ///
    /// 1. Establish the [machine-trap recovery ABI](recovery), including its
    ///    requirements for the handler, linker records, and live trap-return state.
    /// 2. Uphold the hardware and read-effect requirements of [`Self::read_unchecked`].
    #[inline(always)]
    unsafe fn try_read() -> Result<Self::Value, AccessError> {
        // SAFETY: The caller establishes the recovery ABI and register context.
        unsafe { instruction::Recoverable::read::<Self>() }
    }
}

impl<T: ReadSpec> Read for T {}

/// Write access with operands permitted by the CSR definition.
pub trait Write: Csr {
    /// Executes the write operation permitted for `V`.
    ///
    /// A whole-value write suppresses the read of each CSR word.
    /// [`Set`] and [`Clear`] update one word atomically.
    /// Operands that support readback return the previous value.
    ///
    /// On RV32, the intermediate timer comparator values are at least the
    /// smaller of the old and new deadlines.
    /// Counter writes can expose an intermediate value to other readers.
    /// Stop accumulation first if the replacement counter value must be exact.
    ///
    /// # Safety
    ///
    /// 1. Establish that the CSR is accessible on the intended hart.
    ///    For a vendor CSR, confirm its meaning on this core model and revision.
    /// 2. Satisfy the register's write requirements for the operand and any
    ///    intermediate RV32 values, and serialize conflicting updates.
    #[inline(always)]
    unsafe fn write<V>(value: V) -> <Self as WriteSpec<V>>::Output
    where
        Self: WriteSpec<V>,
    {
        // SAFETY: The caller owns the register effects and synchronization.
        unsafe { instruction::Direct::write::<Self, V>(value) }
            .expect("unprotected CSR writes cannot return a recovery error")
    }

    /// Executes a permitted write and returns [`AccessError::IllegalInstruction`] for an illegal access.
    ///
    /// If an RV32 multiword write fails, any earlier writes remain in effect.
    ///
    /// # Safety
    ///
    /// 1. Establish the [machine-trap recovery ABI](recovery).
    /// 2. Uphold [`Self::write`]'s requirements for any implemented CSR.
    ///    The CSR or an RV32 high word may be absent.
    #[inline(always)]
    unsafe fn try_write<V>(value: V) -> Result<<Self as WriteSpec<V>>::Output, AccessError>
    where
        Self: WriteSpec<V>,
    {
        // SAFETY: The caller establishes recovery and coordinates register effects.
        unsafe { instruction::Recoverable::write::<Self, V>(value) }
    }
}

impl<T: Csr> Write for T {}

/// Read-write access to an IMSIC interrupt claim or an entropy-source poll.
pub trait Claim: ClaimSpec {
    /// Claims the top external interrupt or polls the entropy source.
    ///
    /// A single `csrrw` reads the old value and writes zero.
    /// For [IMSIC](https://docs.riscv.org/reference/aia/v1.0/IMSIC.html#2-1-9-top-external-interrupt-csrs-mtopei-stopei-vstopei),
    /// this clears the pending bit of the reported interrupt, if any.
    /// For [`seed`](https://docs.riscv.org/reference/isa/v20260120/unpriv/scalar-crypto.html#crypto_scalar_seed_csr),
    /// a poll consumes an entropy sample when it returns `ES16`.
    ///
    /// # Safety
    ///
    /// Execute where the interrupt file or entropy source is accessible.
    /// Own the interrupt acknowledgment or entropy sample consumed by this operation.
    #[inline(always)]
    unsafe fn claim() -> Self::Value {
        const { instruction::assert_supported_width(Self::WIDTH) }
        let value: usize;
        // SAFETY: The caller owns the claim and its read/write effects.
        unsafe {
            core::arch::asm!("csrrw {}, {}, x0", out(reg) value, const Self::NUMBER, options(nostack))
        };
        Self::from_claim_raw(value)
    }
}

impl<T: ClaimSpec> Claim for T {}

/// Exception reported by a recoverable CSR instruction.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AccessError {
    /// An illegal-instruction exception, such as an access to an unavailable CSR.
    IllegalInstruction,
}

impl core::fmt::Display for AccessError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("CSR access raised an illegal-instruction exception")
    }
}

impl core::error::Error for AccessError {}
