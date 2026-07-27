//! Whole-machine reset protocol adapter.

use machine::power::{self, PowerError, PowerReason, RebootKind};
use rustsbi::SbiRet;
use sbi_spec::binary::Error;
use sbi_spec::srst;

pub(super) struct Reset;

impl Reset {
    pub(super) const fn new() -> Self {
        Self
    }
}

impl rustsbi::Reset for Reset {
    fn system_reset(&self, reset_type: u32, reset_reason: u32) -> SbiRet {
        let reason = match reset_reason {
            srst::RESET_REASON_NO_REASON => PowerReason::Unspecified,
            srst::RESET_REASON_SYSTEM_FAILURE => PowerReason::SystemFailure,
            _ => return Error::InvalidParam.into(),
        };
        let error = match reset_type {
            srst::RESET_TYPE_SHUTDOWN => power::shutdown(reason),
            srst::RESET_TYPE_COLD_REBOOT => power::reboot(RebootKind::Cold, reason),
            srst::RESET_TYPE_WARM_REBOOT => power::reboot(RebootKind::Warm, reason),
            _ => return Error::InvalidParam.into(),
        };
        match error {
            Err(PowerError::Unsupported) => Error::NotSupported.into(),
            Ok(never) => match never {},
        }
    }
}
