//! Whole-machine reset protocol adapter.

use machine::{Power, PowerError, PowerReason, RebootKind};
use rustsbi::SbiRet;
use sbi_spec::srst;

pub(super) struct Reset {
    power: Power,
}

impl Reset {
    pub(super) fn new(power: Power) -> Self {
        Self { power }
    }
}

impl rustsbi::Reset for Reset {
    fn system_reset(&self, reset_type: u32, reset_reason: u32) -> SbiRet {
        let reason = match reset_reason {
            srst::RESET_REASON_NO_REASON => PowerReason::Unspecified,
            srst::RESET_REASON_SYSTEM_FAILURE => PowerReason::SystemFailure,
            _ => return SbiRet::invalid_param(),
        };
        let error = match reset_type {
            srst::RESET_TYPE_SHUTDOWN => self.power.shutdown(reason),
            srst::RESET_TYPE_COLD_REBOOT => self.power.reboot(RebootKind::Cold, reason),
            srst::RESET_TYPE_WARM_REBOOT => self.power.reboot(RebootKind::Warm, reason),
            _ => return SbiRet::invalid_param(),
        };
        match error {
            PowerError::Unsupported => SbiRet::not_supported(),
        }
    }
}
