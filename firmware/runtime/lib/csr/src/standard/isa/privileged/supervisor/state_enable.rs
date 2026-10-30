//! Supervisor state-enable registers.
//!
//! See <https://docs.riscv.org/reference/isa/priv/smstateen.html>.
use crate::{macros::csr, values::StateEnableSet};

csr! {
    /// Supervisor state-enable register zero (`sstateen0`).
    ///
    /// See <https://docs.riscv.org/reference/isa/priv/smstateen.html>.
    StateEnable0 { 0x10c, r: StateEnableSet,
        /// # Safety
        ///
        /// Before user execution, ensure the configured permissions are valid for user execution
        /// and coordinate changes with other users of this hart's state.
        unsafe w: StateEnableSet
    }
    StateEnable1 { 0x10d, r: StateEnableSet,
        /// # Safety
        ///
        /// Before user execution, ensure the configured permissions are valid for user execution
        /// and coordinate changes with other users of this hart's state.
        unsafe w: StateEnableSet
    }
    StateEnable2 { 0x10e, r: StateEnableSet,
        /// # Safety
        ///
        /// Before user execution, ensure the configured permissions are valid for user execution
        /// and coordinate changes with other users of this hart's state.
        unsafe w: StateEnableSet
    }
    StateEnable3 { 0x10f, r: StateEnableSet,
        /// # Safety
        ///
        /// Before user execution, ensure the configured permissions are valid for user execution
        /// and coordinate changes with other users of this hart's state.
        unsafe w: StateEnableSet
    }
}
