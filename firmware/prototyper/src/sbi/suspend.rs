//! System suspend.
//!
//! # References
//!
//! - Specification: [RISC-V SBI SUSP extension](https://docs.riscv.org/reference/sbi/v3.0/ext-sys-suspend.html) —
//!   sleep types, entry requirements, and resume state.

#![forbid(unsafe_code)]

use riscv::register::mstatus;
use runtime::rustsbi::{Hsm, SbiRet};
use sbi_spec::hsm::suspend_type::NON_RETENTIVE;

use runtime::hart::{self, HartId, HartState};

const SUSPEND_TO_RAM: u32 = 0x0;

/// Implementation of SBI System Suspend Extension extension.
pub(crate) struct SbiSuspend;

impl runtime::rustsbi::Susp for SbiSuspend {
    fn system_suspend(&self, sleep_type: u32, resume_addr: usize, opaque: usize) -> SbiRet {
        if sleep_type != SUSPEND_TO_RAM {
            return SbiRet::invalid_param();
        }

        let prev_mode = mstatus::read().mpp();
        if prev_mode != mstatus::MPP::Supervisor && prev_mode != mstatus::MPP::User {
            return SbiRet::failed();
        }

        // Check if all harts except the current hart are stopped
        let hart_enable_map = if let Some(hart_enable_map) = crate::platform::enabled_harts() {
            hart_enable_map
        } else {
            return SbiRet::failed();
        };
        let current_hart = HartId::current()
            .expect("BUG: current hart exceeds Runtime capacity")
            .as_usize();
        for (hartid, hart_enable) in hart_enable_map.iter().enumerate() {
            if *hart_enable && hartid != current_hart {
                let hart = HartId::from_raw(hartid)
                    .expect("BUG: enabled-hart policy exceeds Runtime capacity");
                if hart::status(hart) != HartState::Stopped {
                    return SbiRet::denied();
                }
            }
        }

        // TODO: The validity of `resume_addr` should be checked.
        // If it is invalid, `SBI_ERR_INVALID_ADDRESS` should be returned.

        match crate::sbi::hsm() {
            Some(hsm) => hsm.hart_suspend(NON_RETENTIVE, resume_addr, opaque),
            None => SbiRet::not_supported(),
        }
    }
}
