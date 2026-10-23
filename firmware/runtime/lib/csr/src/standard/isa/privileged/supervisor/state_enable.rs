//! Supervisor state-enable registers.
use crate::{macros::csr, values::StateEnableSet};

csr! {
    /// Supervisor state-enable register zero (`sstateen0`).
    ///
    /// See <https://docs.riscv.org/reference/isa/priv/smstateen.html>.
    StateEnable0 { 0x10c, r: StateEnableSet,
        /// # Safety
        /// Own this hart's user-state permissions before user execution.
        unsafe w: StateEnableSet
    }
    StateEnable1 { 0x10d, r: StateEnableSet,
        /// # Safety
        /// Own this hart's user-state permissions before user execution.
        unsafe w: StateEnableSet
    }
    StateEnable2 { 0x10e, r: StateEnableSet,
        /// # Safety
        /// Own this hart's user-state permissions before user execution.
        unsafe w: StateEnableSet
    }
    StateEnable3 { 0x10f, r: StateEnableSet,
        /// # Safety
        /// Own this hart's user-state permissions before user execution.
        unsafe w: StateEnableSet
    }
}
