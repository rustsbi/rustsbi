//! Guest counter access controls.

use crate::standard::isa::values::counter::CounterSet;

define_csr! {
    /// Hypervisor counter-enable register (`hcounteren`).
    ///
    /// # Access requirements
    ///
    /// Guest counter reads require the corresponding `mcounteren` and
    /// `hcounteren` bits. VU reads also require the `scounteren` bit.
    ///
    /// Reference: <https://docs.riscv.org/reference/isa/v20260120/priv/hypervisor.html#14-1-2-6-hypervisor-counter-enable-hcounteren-register>.
    Hcounteren { 0x606, r: CounterSet, w: CounterSet };
}
