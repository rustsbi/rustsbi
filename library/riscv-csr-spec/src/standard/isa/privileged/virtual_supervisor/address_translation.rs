//! Virtual-supervisor address translation and protection.

use crate::standard::isa::values::address::AddressSpace;

define_csr! {
    /// Virtual supervisor address translation and protection (`vsatp`).
    ///
    /// # Access requirements
    ///
    /// Preserve mappings used by the guest's active VS-stage accesses.
    /// `HFENCE.VVMA` synchronizes changes for the current VMID.
    /// For an unsupported `MODE`, hardware may ignore a write with `V = 0`
    /// or select legal field values. With `V = 1`, a write through guest `satp`
    /// with an unsupported `MODE` leaves the entire register unchanged.
    ///
    /// Reference: <https://docs.riscv.org/reference/isa/v20260120/priv/hypervisor.html#14-1-2-18-virtual-supervisor-address-translation-and-protection-vsatp-register>.
    Vsatp { 0x280, r: AddressSpace, w: AddressSpace };
}
