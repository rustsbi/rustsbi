//! Supervisor address translation and protection.

use crate::standard::isa::values::address::AddressSpace;

define_csr! {
    /// Supervisor address translation and protection (`satp`).
    ///
    /// # Access requirements
    ///
    /// Preserve mappings required by active S/U accesses. Writing `satp`
    /// neither orders page-table stores nor invalidates cached translations.
    /// Use the required `SFENCE.VMA` when changing tables or reusing an ASID.
    /// A write with an unsupported MODE leaves the entire register unchanged.
    Satp { 0x180, r: AddressSpace, w: AddressSpace };
}
