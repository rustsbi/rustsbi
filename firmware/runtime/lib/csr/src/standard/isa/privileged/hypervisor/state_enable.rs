//! Hypervisor state-enable registers.
use crate::{macros::csr, values::StateEnableSet};

csr! {
    /// Hypervisor state-enable register zero (`hstateen0`).
    ///
    /// See <https://docs.riscv.org/reference/isa/priv/smstateen.html>.
    StateEnable0 { 0x60c, r: StateEnableSet }
}
