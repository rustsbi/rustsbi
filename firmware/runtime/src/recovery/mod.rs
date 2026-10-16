//! Static instruction/cause records shared by fallible hardware operations.
//!
//! Each record stores signed instruction and continuation offsets relative to
//! their own fields, followed by a synchronous-exception bit mask.
//! Dispatch recovers only machine-origin traps with an exact PC and accepted cause.
//! It resumes at the continuation with `t2 = mcause` and `t3 = mtval`.
//! Callers initialize `t2` to zero and declare `t2` and `t3` as assembly outputs.

mod instruction;

#[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
pub(crate) use instruction::unavailable;
pub(crate) use instruction::{csrr, csrrw, csrw, instruction};

#[repr(C)]
struct FaultSite {
    instruction: i32,
    fixup: i32,
    causes: u32,
}

impl FaultSite {
    fn fixup(&self, pc: usize, cause: usize) -> Option<usize> {
        if cause >= u32::BITS as usize {
            return None;
        }
        let instruction =
            (&raw const self.instruction as usize).wrapping_add_signed(self.instruction as isize);
        (pc == instruction && self.causes & (1u32 << cause) != 0)
            .then(|| (&raw const self.fixup as usize).wrapping_add_signed(self.fixup as isize))
    }
}

/// Returns the registered continuation for a fault PC and accepted cause.
// Dispatch restricts recovery to machine-origin exceptions.
pub(crate) fn find_fixup(pc: usize, cause: usize) -> Option<usize> {
    #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
    {
        unsafe extern "C" {
            static __trap_fixup_start: FaultSite;
            static __trap_fixup_end: FaultSite;
        }
        let start = &raw const __trap_fixup_start;
        let bytes = (&raw const __trap_fixup_end as usize) - start as usize;
        // SAFETY: The linker collects aligned, immutable FaultSite records.
        let sites = unsafe { core::slice::from_raw_parts(start, bytes / size_of::<FaultSite>()) };
        sites.iter().find_map(|site| site.fixup(pc, cause))
    }
    #[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
    {
        let _ = (pc, cause);
        None
    }
}

/// Emits a relocatable fault record for assembly labels `2` and `3`.
///
/// Label `2` marks the faulting instruction; label `3` marks its continuation.
/// Both precede this macro. The enclosing assembly supplies `{causes}` and
/// follows this module's `t2`/`t3` output protocol.
#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
macro_rules! fault_site {
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

#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
pub(crate) use fault_site;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixup_requires_exact_pc_and_accepted_synchronous_cause() {
        let site = FaultSite {
            instruction: -16,
            fixup: 24,
            causes: 1 << 2,
        };
        let instruction = (&raw const site.instruction as usize).wrapping_sub(16);
        let fixup = (&raw const site.fixup as usize).wrapping_add(24);
        assert_eq!(site.fixup(instruction, 2), Some(fixup));
        assert_eq!(site.fixup(instruction + 4, 2), None);
        for cause in [0, 5, 32, usize::MAX, (1 << (usize::BITS - 1)) | 2] {
            assert_eq!(site.fixup(instruction, cause), None);
        }
    }
}
