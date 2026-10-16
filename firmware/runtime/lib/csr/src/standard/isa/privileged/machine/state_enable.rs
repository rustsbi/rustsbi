//! Machine state-enable registers.
use crate::{macros::csr, values::StateEnableSet};

csr! {
    /// State access permitted to lower privilege modes (`mstateen0`).
    ///
    /// <https://docs.riscv.org/reference/isa/priv/smstateen.html>
    StateEnable0 { 0x30c, r: StateEnableSet }
}
