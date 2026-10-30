//! System suspend.
//!
//! # References
//!
//! - Specification: [RISC-V SBI SUSP extension](https://docs.riscv.org/reference/sbi/v3.0/ext-sys-suspend.html) —
//!   sleep types, entry requirements, and resume state.

#![forbid(unsafe_code)]

use runtime::{
    Result,
    hart::{Hart, HartId, HartState, TargetPrivilege},
    rustsbi::{Hsm, SbiRet, Susp},
    trap,
};
use sbi_spec::hsm::suspend_type::NON_RETENTIVE;

const SUSPEND_TO_RAM: u32 = 0x0;

/// Implementation of SBI System Suspend Extension extension.
pub(crate) struct SbiSuspend;

impl Susp for SbiSuspend {
    fn system_suspend(&self, sleep_type: u32, resume_addr: usize, opaque: usize) -> SbiRet {
        if sleep_type != SUSPEND_TO_RAM {
            return SbiRet::invalid_param();
        }

        let prev_mode = trap::previous_privilege();
        if !matches!(
            prev_mode,
            Some(TargetPrivilege::Supervisor | TargetPrivilege::User)
        ) {
            return SbiRet::failed();
        }

        if let Err(error) = check_other_harts_stopped() {
            return error;
        }

        // TODO: The validity of `resume_addr` should be checked.
        // If it is invalid, `SBI_ERR_INVALID_ADDRESS` should be returned.

        match crate::sbi::hsm() {
            Some(hsm) => hsm.hart_suspend(NON_RETENTIVE, resume_addr, opaque),
            None => SbiRet::not_supported(),
        }
    }
}

fn check_other_harts_stopped() -> Result<(), SbiRet> {
    let current = HartId::current().map_err(SbiRet::from)?;
    let harts = crate::platform::enabled_harts().ok_or_else(SbiRet::failed)?;
    for id in harts.filter(|id| *id != current) {
        let hart = Hart::get(id).map_err(SbiRet::from)?;
        if hart.status() != HartState::Stopped {
            return Err(SbiRet::denied());
        }
    }
    Ok(())
}
