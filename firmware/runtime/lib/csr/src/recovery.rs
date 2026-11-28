//! ABI records for recoverable hardware instructions.
//!
//! The firmware linker must retain `.trap_fixup`.
//! For an exception originating in M-mode, the handler matches the fault PC
//! and cause against a [`FaultSite`].
//! On a match it resumes at the continuation with `t2 = mcause` and `t3 = mtval`.
//!
//! Enter in M-mode with `MPRV` clear, an ABI-aligned stack with room for the
//! handler, and machine traps deliverable (`mstatus.MDT = 0` on Smdbltrp harts).
//! The handler must preserve integer registers other than `t2`, `t3`, and declared
//! outputs, as well as any live floating-point or vector state.
//!
//! A recovered exception can overwrite `mepc`, `mcause`, `mtval`, and any
//! implemented `mtinst` and `mtval2`.
//! Trap entry and return also change applicable `mstatus` and `mstatush` fields.
//! Finish probes before preparing a trap-return context, or save and restore
//! any fields that context needs.

// Only the CSR between labels 2 and 3 may be skipped by the trap handler.
macro_rules! try_csr_asm {
    ($instruction:literal, $number:expr, $($operands:tt)*) => {{
        let cause: usize;
        core::arch::asm!(
            "2:",
            $instruction,
            "3:",
            $crate::recovery_record!(),
            csr = const $number,
            causes = const 1 << 2,
            $($operands)*
            inout("t2") 0usize => cause,
            out("t3") _,
        );
        if cause == 0 {
            Ok(())
        } else {
            Err($crate::AccessError::IllegalInstruction)
        }
    }};
}
pub(crate) use try_csr_asm;

/// PC-relative instruction, continuation, and accepted-exception record.
///
/// Each signed 32-bit offset is relative to the address of its own field.
/// The final word is a bitmask indexed by synchronous exception code.
/// Linker records must be four-byte aligned.
#[repr(C)]
pub struct FaultSite {
    instruction: i32,
    fixup: i32,
    causes: u32,
}

impl FaultSite {
    /// Returns the continuation if both the fault PC and synchronous exception code match.
    pub fn fixup(&self, pc: usize, cause: usize) -> Option<usize> {
        if cause >= u32::BITS as usize {
            return None;
        }
        let instruction =
            (&raw const self.instruction as usize).wrapping_add_signed(self.instruction as isize);
        (pc == instruction && self.causes & (1u32 << cause) != 0)
            .then(|| (&raw const self.fixup as usize).wrapping_add_signed(self.fixup as isize))
    }
}

/// Emits a `.trap_fixup` record for assembly labels `2` and `3`.
///
/// Label `2` identifies the protected instruction and label `3` its continuation.
/// Both labels must precede this template and be within signed 32-bit reach of
/// their respective record fields.
/// Supply the exception-code mask as `{causes}`, initialize `t2` to zero, and
/// declare `t2` and `t3` as outputs.
/// Allow the handler to use the current stack and memory as required by the
/// [recovery ABI](crate::recovery).
#[macro_export]
macro_rules! recovery_record {
    () => {
        concat!(
            ".pushsection .trap_fixup, \"a\", @progbits\n",
            ".balign 4\n",
            ".long 2b - .\n",
            ".long 3b - .\n",
            ".long {causes}\n",
            ".popsection\n",
        )
    };
}
