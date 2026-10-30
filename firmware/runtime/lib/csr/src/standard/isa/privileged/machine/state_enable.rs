//! Machine state-enable registers.
//!
//! See <https://docs.riscv.org/reference/isa/priv/smstateen.html>.
//! The AIA-controlled fields are specified in
//! <https://docs.riscv.org/reference/aia/v1.0/CSRs.html>.
use crate::{macros::csr, values::StateEnableSet};
use super::FeatureState;

csr! {
    /// Machine state-enable register zero (`mstateen0`).
    ///
    /// See <https://docs.riscv.org/reference/isa/priv/smstateen.html>.
    StateEnable0 { 0x30c, r,
        /// # Safety
        ///
        /// During initial setup, ensure the configured permissions are valid for lower-mode execution
        /// and coordinate changes with other users of this hart's state.
        unsafe w
    }
        #[cfg(target_pointer_width = "64")]
        CONTEXT[57] { rw: FeatureState }
        #[cfg(target_pointer_width = "64")]
        IMSIC[58] { rw: FeatureState }
        #[cfg(target_pointer_width = "64")]
        AIA[59] { rw: FeatureState }
        #[cfg(target_pointer_width = "64")]
        CSRIND[60] { rw: FeatureState }
        #[cfg(target_pointer_width = "64")]
        ENVCFG[62] { rw: FeatureState }
        #[cfg(target_pointer_width = "64")]
        SE0[63] { rw: FeatureState }
    StateEnable1 { 0x30d, r: StateEnableSet,
        /// # Safety
        ///
        /// During initial setup, ensure the configured permissions are valid for lower-mode execution
        /// and coordinate changes with other users of this hart's state.
        unsafe w: StateEnableSet
    }
    StateEnable2 { 0x30e, r: StateEnableSet,
        /// # Safety
        ///
        /// During initial setup, ensure the configured permissions are valid for lower-mode execution
        /// and coordinate changes with other users of this hart's state.
        unsafe w: StateEnableSet
    }
    StateEnable3 { 0x30f, r: StateEnableSet,
        /// # Safety
        ///
        /// During initial setup, ensure the configured permissions are valid for lower-mode execution
        /// and coordinate changes with other users of this hart's state.
        unsafe w: StateEnableSet
    }
    StateEnable0High { 0x31c, r,
        /// # Safety
        ///
        /// During initial setup, ensure the configured permissions are valid for lower-mode execution
        /// and coordinate changes with other users of this hart's state.
        unsafe w
    }
        CONTEXTH[25] { rw: FeatureState }
        IMSICH[26] { rw: FeatureState }
        AIAH[27] { rw: FeatureState }
        CSRINDH[28] { rw: FeatureState }
        ENVCFGH[30] { rw: FeatureState }
        SE0H[31] { rw: FeatureState }
    StateEnable1High { 0x31d, r: StateEnableSet,
        /// # Safety
        ///
        /// During initial setup, ensure the configured permissions are valid for lower-mode execution
        /// and coordinate changes with other users of this hart's state.
        unsafe w: StateEnableSet
    }
    StateEnable2High { 0x31e, r: StateEnableSet,
        /// # Safety
        ///
        /// During initial setup, ensure the configured permissions are valid for lower-mode execution
        /// and coordinate changes with other users of this hart's state.
        unsafe w: StateEnableSet
    }
    StateEnable3High { 0x31f, r: StateEnableSet,
        /// # Safety
        ///
        /// During initial setup, ensure the configured permissions are valid for lower-mode execution
        /// and coordinate changes with other users of this hart's state.
        unsafe w: StateEnableSet
    }
}
