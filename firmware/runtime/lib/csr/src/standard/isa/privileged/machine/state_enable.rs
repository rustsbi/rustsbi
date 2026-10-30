//! Machine state-enable registers.
use crate::{macros::csr, values::StateEnableSet};
use super::FeatureState;

csr! {
    /// Machine state-enable register zero (`mstateen0`).
    ///
    /// See <https://docs.riscv.org/reference/isa/priv/smstateen.html>.
    StateEnable0 { 0x30c, r,
        /// # Safety
        /// Own this hart's lower-mode state permissions during initial setup.
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
        /// Own this hart's lower-mode state permissions during initial setup.
        unsafe w: StateEnableSet
    }
    StateEnable2 { 0x30e, r: StateEnableSet,
        /// # Safety
        /// Own this hart's lower-mode state permissions during initial setup.
        unsafe w: StateEnableSet
    }
    StateEnable3 { 0x30f, r: StateEnableSet,
        /// # Safety
        /// Own this hart's lower-mode state permissions during initial setup.
        unsafe w: StateEnableSet
    }
    #[cfg(target_pointer_width = "32")]
    StateEnable0High { 0x31c, r,
        /// # Safety
        /// Own this hart's lower-mode state permissions during initial setup.
        unsafe w
    }
        CONTEXTH[25] { rw: FeatureState }
        IMSICH[26] { rw: FeatureState }
        AIAH[27] { rw: FeatureState }
        CSRINDH[28] { rw: FeatureState }
        ENVCFGH[30] { rw: FeatureState }
        SE0H[31] { rw: FeatureState }
    #[cfg(target_pointer_width = "32")]
    StateEnable1High { 0x31d, r: StateEnableSet,
        /// # Safety
        /// Own this hart's lower-mode state permissions during initial setup.
        unsafe w: StateEnableSet
    }
    #[cfg(target_pointer_width = "32")]
    StateEnable2High { 0x31e, r: StateEnableSet,
        /// # Safety
        /// Own this hart's lower-mode state permissions during initial setup.
        unsafe w: StateEnableSet
    }
    #[cfg(target_pointer_width = "32")]
    StateEnable3High { 0x31f, r: StateEnableSet,
        /// # Safety
        /// Own this hart's lower-mode state permissions during initial setup.
        unsafe w: StateEnableSet
    }
}

impl StateEnable0WriteValue {
    /// Closes every writable permission in the low word.
    pub const fn closed() -> Self {
        Self(0)
    }
}

#[cfg(target_pointer_width = "32")]
impl StateEnable0HighWriteValue {
    /// Closes every writable permission in the high word.
    pub const fn closed() -> Self {
        Self(0)
    }
}
