//! System reset.
//!
//! # References
//!
//! - Specification: [RISC-V SBI SRST extension](https://docs.riscv.org/reference/sbi/v3.0/ext-sys-reset.html)
//!   — reset types, reasons, and error semantics.

#![forbid(unsafe_code)]

use alloc::{boxed::Box, vec::Vec};
use spin::Mutex;

use crate::driver::{ResetDevice, ResetError, ResetReason, ResetRequest, ResetType};
use runtime::rustsbi;
use rustsbi::SbiRet;

/// SBI system-reset extension over the bound reset devices.
#[derive(Default)]
pub(crate) struct SbiReset {
    devices: Vec<Mutex<Box<dyn ResetDevice>>>,
}

impl SbiReset {
    pub(crate) fn new(mut devices: Vec<Box<dyn ResetDevice>>) -> Self {
        devices.sort_by_key(|device| core::cmp::Reverse(device.priority()));
        Self {
            devices: devices.into_iter().map(Mutex::new).collect(),
        }
    }
}

impl rustsbi::Reset for SbiReset {
    fn _rustsbi_probe(&self) -> usize {
        usize::from(!self.devices.is_empty())
    }

    fn system_reset(&self, reset_type: u32, reset_reason: u32) -> SbiRet {
        let Some(request) = parse_request(reset_type, reset_reason) else {
            return SbiRet::invalid_param();
        };
        if self.devices.is_empty() {
            return SbiRet::not_supported();
        }
        for device in &self.devices {
            match device.lock().system_reset(request) {
                ResetError::InvalidRequest => continue,
                ResetError::Failed => return SbiRet::failed(),
            }
        }
        SbiRet::invalid_param()
    }
}

/// Validates both raw parameters before querying or invoking a reset backend.
fn parse_request(reset_type: u32, reset_reason: u32) -> Option<ResetRequest> {
    use rustsbi::spec::srst::{
        RESET_REASON_NO_REASON, RESET_REASON_SYSTEM_FAILURE, RESET_TYPE_COLD_REBOOT,
        RESET_TYPE_SHUTDOWN, RESET_TYPE_WARM_REBOOT,
    };

    let reset_type = match reset_type {
        RESET_TYPE_SHUTDOWN => ResetType::Shutdown,
        RESET_TYPE_COLD_REBOOT => ResetType::ColdReboot,
        RESET_TYPE_WARM_REBOOT => ResetType::WarmReboot,
        0xf000_0000..=0xffff_ffff => ResetType::VendorSpecific(reset_type),
        _ => return None,
    };
    let reset_reason = match reset_reason {
        RESET_REASON_NO_REASON => ResetReason::NoReason,
        RESET_REASON_SYSTEM_FAILURE => ResetReason::SystemFailure,
        0xe000_0000..=0xefff_ffff => ResetReason::SbiSpecific(reset_reason),
        0xf000_0000..=0xffff_ffff => ResetReason::VendorSpecific(reset_reason),
        _ => return None,
    };
    Some(ResetRequest::new(reset_type, reset_reason))
}

#[allow(unused)]
pub(crate) fn fail() -> ! {
    use rustsbi::spec::srst::{
        EID_SRST, RESET_REASON_SYSTEM_FAILURE, RESET_TYPE_SHUTDOWN, SYSTEM_RESET,
    };

    if let Some(dispatcher) = super::SBI_DISPATCHER.get() {
        let ret = rustsbi::RustSBI::handle_ecall(
            dispatcher,
            EID_SRST,
            SYSTEM_RESET,
            [
                RESET_TYPE_SHUTDOWN as usize,
                RESET_REASON_SYSTEM_FAILURE as usize,
                0,
                0,
                0,
                0,
            ],
        );
        error!("System failure reset returned: {ret:?}");
    }
    crate::fail::stop()
}
