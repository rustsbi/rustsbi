//! Private instructions used by Runtime operations.

pub(crate) mod fence {
    use core::arch::asm;

    #[inline]
    pub(crate) fn sfence_vma_all() {
        riscv::asm::sfence_vma_all();
    }

    #[inline]
    pub(crate) fn hfence_gvma_all() {
        // Enable H only for assembling this instruction; callers check H
        // before execution, so the rest of the image can run without H.
        // SAFETY:
        // 1. The owning Runtime operation executes in M-mode.
        // 2. It checks this hart's H extension before reaching this instruction.
        unsafe {
            asm!(
                ".option push",
                ".option arch, +h",
                "hfence.gvma x0, x0",
                ".option pop",
                options(nostack)
            )
        };
    }
}
