//! Word access protocols for [Zicsr](https://docs.riscv.org/reference/isa/v20260120/unpriv/zicsr.html).
//!
//! On RV32 a logical 64-bit register can require several instructions.
//! Each instruction has its own recovery point on the recoverable path.

use riscv_csr_spec::{Csr, ReadSpec, Width, WriteOp, WriteSpec};

use crate::AccessError;

pub(super) struct Access<const RECOVER: bool>;

pub(super) type Direct = Access<false>;
pub(super) type Recoverable = Access<true>;

pub(super) const fn assert_supported_width(width: Width) {
    assert!(
        !width.is_rv32_only() || usize::BITS == 32,
        "RV32-only CSR on RV64"
    );
}

// The assembler needs a valid immediate even for a CSR whose upper word is never accessed.
#[cfg(target_pointer_width = "32")]
const fn upper_word<C: Csr + ?Sized>() -> u16 {
    match C::WIDTH {
        Width::Split64 { high } | Width::Counter64 { high } | Width::Timer64 { high } => high,
        Width::Xlen | Width::Rv32Only => C::NUMBER,
    }
}

macro_rules! execute_word {
    ($recover:expr, $opcode:literal, $csr:expr, $($operands:tt)*) => {{
        if $recover {
            crate::recovery::try_csr_asm!($opcode, $csr, $($operands)*)
        } else {
            core::arch::asm!($opcode, csr = const $csr, $($operands)* options(nostack));
            Ok(())
        }
    }};
}

impl<const RECOVER: bool> Access<RECOVER> {
    /// Reads a logical register using its declared word protocol.
    ///
    /// # Safety
    ///
    /// Uphold the CSR's read-effect requirements and, for `RECOVER`, the
    /// [machine trap recovery ABI](crate::recovery).
    #[inline(always)]
    pub(super) unsafe fn read<C: ReadSpec + ?Sized>() -> Result<C::Value, AccessError> {
        const { assert_supported_width(C::WIDTH) }

        macro_rules! read_word {
            ($number:expr) => {{
                let word: usize;
                // SAFETY: The caller permits the read effects of both words and, when selected,
                // supplies the recovery handler for this instruction.
                unsafe { execute_word!(RECOVER, "csrr {word}, {csr}", $number, word = out(reg) word,) }?;
                word
            }};
        }

        #[cfg(target_pointer_width = "64")]
        let bits = read_word!(C::NUMBER) as u64;
        #[cfg(target_pointer_width = "32")]
        let bits = match C::WIDTH {
            Width::Xlen | Width::Rv32Only => read_word!(C::NUMBER) as u64,
            Width::Split64 { .. } | Width::Timer64 { .. } => {
                let low = read_word!(C::NUMBER);
                let high = read_word!(upper_word::<C>());
                ((high as u64) << 32) | low as u64
            }
            Width::Counter64 { .. } => loop {
                let high = read_word!(upper_word::<C>());
                let low = read_word!(C::NUMBER);
                if high == read_word!(upper_word::<C>()) {
                    break ((high as u64) << 32) | low as u64;
                }
            },
        };
        Ok(C::from_raw(bits))
    }

    /// Applies the operand to its CSR and returns any requested readback.
    ///
    /// An RV32 fault can leave earlier words of the same write effective.
    ///
    /// # Safety
    ///
    /// Uphold the CSR's write requirements and, for `RECOVER`, the
    /// [machine trap recovery ABI](crate::recovery).
    #[inline(always)]
    pub(super) unsafe fn write<C: WriteSpec<V> + ?Sized, V>(
        value: V,
    ) -> Result<C::Output, AccessError> {
        const { assert_supported_width(C::WIDTH) }
        let mut previous = 0;
        // SAFETY: WriteSpec selects the permitted opcode and CSR word.
        // The caller owns the effects of every word and supplies recovery when selected.
        unsafe {
            match C::encode_write(value) {
                WriteOp::Replace(bits) => Self::write_whole::<C>(bits),
                WriteOp::Set(mask) => {
                    execute_word!(RECOVER, "csrs {csr}, {mask}", C::ATOMIC_NUMBER, mask = in(reg) mask,)
                }
                WriteOp::Clear(mask) => {
                    execute_word!(RECOVER, "csrc {csr}, {mask}", C::ATOMIC_NUMBER, mask = in(reg) mask,)
                }
                WriteOp::ReadAndClear(mask) => {
                    execute_word!(
                        RECOVER, "csrrc {old}, {csr}, {mask}", C::ATOMIC_NUMBER,
                        old = lateout(reg) previous, mask = in(reg) mask,
                    )
                }
            }?;
        }
        Ok(C::decode_output(previous))
    }

    /// Replaces a logical CSR value, using the register's write order on RV32.
    ///
    /// # Safety
    ///
    /// The CSR must accept whole-value writes.
    /// Uphold the requirements of [`Self::write`] for every word and intermediate value.
    #[inline(always)]
    unsafe fn write_whole<C: Csr + ?Sized>(bits: u64) -> Result<(), AccessError> {
        macro_rules! write_word {
            ($number:expr, $word:expr) => {
                // SAFETY: The caller owns the whole register and the recovery context.
                unsafe { execute_word!(RECOVER, "csrw {csr}, {word}", $number, word = in(reg) $word,) }
            };
        }
        let low = bits as usize;
        #[cfg(target_pointer_width = "64")]
        return write_word!(C::NUMBER, low);

        #[cfg(target_pointer_width = "32")]
        {
            let high = (bits >> 32) as usize;
            match C::WIDTH {
                Width::Xlen | Width::Rv32Only => write_word!(C::NUMBER, low),
                Width::Split64 { .. } => {
                    write_word!(C::NUMBER, low)?;
                    write_word!(upper_word::<C>(), high)
                }
                Width::Counter64 { .. } => {
                    write_word!(upper_word::<C>(), high)?;
                    write_word!(C::NUMBER, low)
                }
                Width::Timer64 { .. } => {
                    write_word!(C::NUMBER, usize::MAX)?;
                    write_word!(upper_word::<C>(), high)?;
                    write_word!(C::NUMBER, low)
                }
            }
        }
    }
}
