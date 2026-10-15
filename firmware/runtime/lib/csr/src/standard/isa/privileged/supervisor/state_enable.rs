//! Supervisor state-enable registers.
use crate::{macros::csr, values::StateEnableSet};

csr! {
    /// Supervisor state-enable register zero (`sstateen0`).
    ///
    /// See <https://docs.riscv.org/reference/isa/priv/smstateen.html>.
    StateEnable0 { 0x10c, r: StateEnableSet }
}
