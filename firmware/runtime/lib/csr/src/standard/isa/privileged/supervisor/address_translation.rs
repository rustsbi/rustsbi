//! Supervisor address translation and protection.
use crate::macros::csr;

csr! {
    pub struct AddressSpace(usize);

    /// Supervisor address translation and protection (`satp`).
    ///
    /// <https://docs.riscv.org/reference/isa/priv/supervisor.html#satp>
    AddressTranslation { 0x180, r: AddressSpace,
        /// See <https://docs.riscv.org/reference/isa/priv/supervisor.html#satp>.
        ///
        /// # Safety
        ///
        /// The new mappings must preserve all live Rust pointers and executable code.
        unsafe w: AddressSpace
    }
}

impl AddressSpace {
    pub const fn bare() -> Self {
        Self(0)
    }
}
