//! Hypervisor state-enable registers.
//!
//! See <https://docs.riscv.org/reference/isa/priv/smstateen.html>.
use crate::{macros::csr, values::StateEnableSet};

macro_rules! state_enable {
    ($( $(#[$attr:meta])* $name:ident = $number:literal; )*) => {
        csr! {
            $(
                $(#[$attr])*
                $name { $number, r: StateEnableSet,
                    /// # Safety
                    ///
                    /// Before guest execution, ensure the configured permissions
                    /// are valid for the guest and coordinate changes with other
                    /// users of this hart's state.
                    unsafe w: StateEnableSet
                }
            )*
        }
    };
}

state_enable! {
    /// Hypervisor state-enable register zero (`hstateen0`).
    StateEnable0 = 0x60c;
    /// Hypervisor state-enable register one (`hstateen1`).
    StateEnable1 = 0x60d;
    /// Hypervisor state-enable register two (`hstateen2`).
    StateEnable2 = 0x60e;
    /// Hypervisor state-enable register three (`hstateen3`).
    StateEnable3 = 0x60f;
    /// Upper half of `hstateen0` on RV32.
    StateEnable0High = 0x61c;
    /// Upper half of `hstateen1` on RV32.
    StateEnable1High = 0x61d;
    /// Upper half of `hstateen2` on RV32.
    StateEnable2High = 0x61e;
    /// Upper half of `hstateen3` on RV32.
    StateEnable3High = 0x61f;
}
