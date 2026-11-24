//! SBI FWFT argument and result adaptation.

#![forbid(unsafe_code)]

use runtime::{
    fwft,
    rustsbi::{Fwft, SbiRet},
};

/// Firmware Features extension.
pub(crate) struct SbiFwft;

impl Fwft for SbiFwft {
    fn set(&self, feature_id: u32, value: usize, flags: usize) -> SbiRet {
        if flags != 0 {
            return SbiRet::invalid_param();
        }
        match fwft::set(feature_id, value) {
            Ok(()) => SbiRet::success(0),
            Err(error) => SbiRet::from(error),
        }
    }

    fn get(&self, feature_id: u32) -> SbiRet {
        match fwft::get(feature_id) {
            Ok(value) => SbiRet::success(value),
            Err(error) => SbiRet::from(error),
        }
    }
}
