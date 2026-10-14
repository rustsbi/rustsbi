//! Supervisor state-enable registers.
use crate::{macros::csr, values::StateEnableSet};

csr! {
    /// State access permitted to user mode (`sstateen0`).
    ///
    /// <https://docs.riscv.org/reference/isa/priv/smstateen.html>
    StateEnable0 { 0x10c, r: StateEnableSet }
}
