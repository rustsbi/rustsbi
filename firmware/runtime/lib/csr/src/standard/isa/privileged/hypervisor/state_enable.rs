//! Hypervisor state-enable registers.
use crate::{macros::csr, values::StateEnableSet};

csr! {
    /// Hypervisor state-enable register zero (`hstateen0`).
    ///
    /// See <https://docs.riscv.org/reference/isa/priv/smstateen.html>.
    StateEnable0 { 0x60c, r: StateEnableSet,
        /// # Safety
        /// Own this hart's guest-state permissions before guest execution.
        unsafe w: StateEnableSet
    }
    StateEnable1 { 0x60d, r: StateEnableSet,
        /// # Safety
        /// Own this hart's guest-state permissions before guest execution.
        unsafe w: StateEnableSet
    }
    StateEnable2 { 0x60e, r: StateEnableSet,
        /// # Safety
        /// Own this hart's guest-state permissions before guest execution.
        unsafe w: StateEnableSet
    }
    StateEnable3 { 0x60f, r: StateEnableSet,
        /// # Safety
        /// Own this hart's guest-state permissions before guest execution.
        unsafe w: StateEnableSet
    }
    #[cfg(target_pointer_width = "32")]
    StateEnable0High { 0x61c, r: StateEnableSet,
        /// # Safety
        /// Own this hart's guest-state permissions before guest execution.
        unsafe w: StateEnableSet
    }
    #[cfg(target_pointer_width = "32")]
    StateEnable1High { 0x61d, r: StateEnableSet,
        /// # Safety
        /// Own this hart's guest-state permissions before guest execution.
        unsafe w: StateEnableSet
    }
    #[cfg(target_pointer_width = "32")]
    StateEnable2High { 0x61e, r: StateEnableSet,
        /// # Safety
        /// Own this hart's guest-state permissions before guest execution.
        unsafe w: StateEnableSet
    }
    #[cfg(target_pointer_width = "32")]
    StateEnable3High { 0x61f, r: StateEnableSet,
        /// # Safety
        /// Own this hart's guest-state permissions before guest execution.
        unsafe w: StateEnableSet
    }
}
