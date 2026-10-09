//! System suspend.
//!
//! # References
//!
//! - Specification: [RISC-V SBI SUSP extension](https://docs.riscv.org/reference/sbi/v3.0/ext-sys-suspend.html) —
//!   sleep types, entry requirements, and resume state.

#![forbid(unsafe_code)]

use runtime::rustsbi::{Hsm, SbiRet};
use sbi_spec::hsm::suspend_type::NON_RETENTIVE;

use runtime::hart::{self, HartId, HartState};

const SUSPEND_TO_RAM: u32 = 0x0;

/// SBI system-suspend adapter using HSM non-retentive resume.
pub(crate) struct SbiSuspend;

impl runtime::rustsbi::Susp for SbiSuspend {
    fn system_suspend(&self, sleep_type: u32, resume_addr: usize, opaque: usize) -> SbiRet {
        if sleep_type != SUSPEND_TO_RAM {
            return SbiRet::invalid_param();
        }

        // Runtime dispatches SBI calls only from supervisor ecalls.
        let hart_enable_map = if let Some(hart_enable_map) = crate::platform::enabled_harts() {
            hart_enable_map
        } else {
            return SbiRet::failed();
        };
        let current_hart = HartId::current().expect("BUG: unknown current hart");
        for hart in hart_enable_map {
            if hart != current_hart && hart::status(hart) != HartState::Stopped {
                return SbiRet::denied();
            }
        }

        // TODO: Validate `resume_addr` and return `SBI_ERR_INVALID_ADDRESS`
        // if it is invalid.

        match crate::sbi::hsm() {
            Some(hsm) => hsm.hart_suspend(NON_RETENTIVE, resume_addr, opaque),
            None => SbiRet::not_supported(),
        }
    }
}
