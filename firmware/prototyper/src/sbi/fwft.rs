//! SBI FWFT argument and result adaptation.

use runtime::{
    Error, fwft,
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
            Err(error) => failure(error),
        }
    }

    fn get(&self, feature_id: u32) -> SbiRet {
        match fwft::get(feature_id) {
            Ok(value) => SbiRet::success(value),
            Err(error) => failure(error),
        }
    }
}

fn failure(error: Error) -> SbiRet {
    match error {
        Error::InvalidArgs => SbiRet::invalid_param(),
        Error::NotSupported => SbiRet::not_supported(),
        _ => SbiRet::failed(),
    }
}
