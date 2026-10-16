//! Hypervisor state-enable registers.
use crate::{macros::csr, values::StateEnableSet};

csr! {
    /// State access permitted to virtual modes (`hstateen0`).
    ///
    /// <https://docs.riscv.org/reference/isa/priv/smstateen.html>
    StateEnable0 { 0x60c, r: StateEnableSet }
}
