//! Address operands shared by architectural CSRs.

csr_value! {
    /// Raw machine or supervisor physical/virtual address operand (`mepc`, `sepc`, `vsepc`, `mconfigptr`).
    ///
    /// Reference: <https://docs.riscv.org/reference/isa/v20260120/priv/machine.html#2-1-1-14-machine-exception-program-counter-mepc-register>.
    #[derive_ord(Ord, PartialOrd)]
    pub struct Address;

    /// Supervisor or virtual-supervisor address translation and protection value (`satp`, `vsatp`).
    pub struct AddressSpace;
}

impl AddressSpace {
    /// Returns the all-zero encoding for Bare address translation.
    #[inline(always)]
    pub const fn bare() -> Self {
        Self(0)
    }
}
