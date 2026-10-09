//! Private instructions used by Runtime operations.

pub(crate) mod fence {
    use core::arch::asm;

    #[inline]
    pub(crate) fn fence_i() {
        riscv::asm::fence_i();
    }

    #[inline]
    pub(crate) fn sfence_vma_all() {
        riscv::asm::sfence_vma_all();
    }

    #[inline]
    pub(crate) fn sfence_vma_addr(addr: usize) {
        // SAFETY: M-mode Runtime invalidates this hart's address translations.
        unsafe { asm!("sfence.vma {}, x0", in(reg) addr, options(nostack)) };
    }

    #[inline]
    pub(crate) fn sfence_vma_asid(asid: usize) {
        // SAFETY: M-mode Runtime invalidates this hart's address translations.
        unsafe { asm!("sfence.vma x0, {}", in(reg) asid, options(nostack)) };
    }

    #[inline]
    pub(crate) fn sfence_vma_addr_asid(addr: usize, asid: usize) {
        // SAFETY: M-mode Runtime invalidates this hart's address translations.
        unsafe { asm!("sfence.vma {}, {}", in(reg) addr, in(reg) asid, options(nostack)) };
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

    #[cfg(feature = "hypervisor")]
    #[inline]
    pub(crate) fn hfence_gvma_addr(addr: usize) {
        // HFENCE.GVMA encodes the byte guest physical address as GPA >> 2:
        // https://docs.riscv.org/reference/isa/v20260120/priv/hypervisor.html#_hypervisor_memory_management_fence_instructions
        // SAFETY:
        // 1. The owning Runtime operation executes in M-mode.
        // 2. It checks this hart's H extension before reaching this instruction.
        unsafe {
            asm!(".option push", ".option arch, +h", "hfence.gvma {}, x0", ".option pop", in(reg) (addr >> 2), options(nostack))
        };
    }

    #[cfg(feature = "hypervisor")]
    #[inline]
    pub(crate) fn hfence_gvma_vmid(vmid: usize) {
        // SAFETY:
        // 1. The owning Runtime operation executes in M-mode.
        // 2. It checks this hart's H extension before reaching this instruction.
        unsafe {
            asm!(".option push", ".option arch, +h", "hfence.gvma x0, {}", ".option pop", in(reg) vmid, options(nostack))
        };
    }

    #[cfg(feature = "hypervisor")]
    #[inline]
    pub(crate) fn hfence_gvma_addr_vmid(addr: usize, vmid: usize) {
        // SAFETY:
        // 1. The owning Runtime operation executes in M-mode.
        // 2. It checks this hart's H extension before reaching this instruction.
        unsafe {
            asm!(".option push", ".option arch, +h", "hfence.gvma {}, {}", ".option pop", in(reg) (addr >> 2), in(reg) vmid, options(nostack))
        };
    }

    #[cfg(feature = "hypervisor")]
    #[inline]
    pub(crate) fn hfence_vvma_all() {
        // SAFETY:
        // 1. The owning Runtime operation executes in M-mode.
        // 2. It checks this hart's H extension before reaching this instruction.
        unsafe {
            asm!(
                ".option push",
                ".option arch, +h",
                "hfence.vvma x0, x0",
                ".option pop",
                options(nostack)
            )
        };
    }

    #[cfg(feature = "hypervisor")]
    #[inline]
    pub(crate) fn hfence_vvma_addr(addr: usize) {
        // SAFETY:
        // 1. The owning Runtime operation executes in M-mode.
        // 2. It checks this hart's H extension before reaching this instruction.
        unsafe {
            asm!(".option push", ".option arch, +h", "hfence.vvma {}, x0", ".option pop", in(reg) addr, options(nostack))
        };
    }

    #[cfg(feature = "hypervisor")]
    #[inline]
    pub(crate) fn hfence_vvma_asid(asid: usize) {
        // SAFETY:
        // 1. The owning Runtime operation executes in M-mode.
        // 2. It checks this hart's H extension before reaching this instruction.
        unsafe {
            asm!(".option push", ".option arch, +h", "hfence.vvma x0, {}", ".option pop", in(reg) asid, options(nostack))
        };
    }

    #[cfg(feature = "hypervisor")]
    #[inline]
    pub(crate) fn hfence_vvma_addr_asid(addr: usize, asid: usize) {
        // SAFETY:
        // 1. The owning Runtime operation executes in M-mode.
        // 2. It checks this hart's H extension before reaching this instruction.
        unsafe {
            asm!(".option push", ".option arch, +h", "hfence.vvma {}, {}", ".option pop", in(reg) addr, in(reg) asid, options(nostack))
        };
    }
}

pub(crate) fn memory_to_io() {
    #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
    // SAFETY: FENCE is valid in M-mode and does not dereference memory.
    unsafe {
        core::arch::asm!("fence w, o", options(nostack))
    };
    #[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
    unimplemented!("machine I/O ordering requires a RISC-V hart");
}

pub(crate) fn io_to_memory() {
    #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
    // SAFETY: FENCE is valid in M-mode and does not dereference memory.
    unsafe {
        core::arch::asm!("fence io, rw", options(nostack))
    };
    #[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
    unimplemented!("machine I/O ordering requires a RISC-V hart");
}
