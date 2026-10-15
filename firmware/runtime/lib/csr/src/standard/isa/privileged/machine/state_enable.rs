//! Machine state-enable registers.
use crate::{macros::csr, values::StateEnableSet};

csr! {
    /// Machine state-enable register zero (`mstateen0`).
    ///
    /// See <https://docs.riscv.org/reference/isa/priv/smstateen.html>.
    StateEnable0 { 0x30c, r: StateEnableSet }
}
