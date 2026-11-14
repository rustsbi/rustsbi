//! System suspend.
//!
//! # References
//!
//! - Specification: [RISC-V SBI SUSP extension](https://docs.riscv.org/reference/sbi/v3.0/ext-sys-suspend.html) —
//!   sleep types, entry requirements, and resume state.

#![forbid(unsafe_code)]

use csr::machine::{MPP, Privilege, Status};
use runtime::{
    hart::{self, HartId, HartState},
    rustsbi::{Hsm, SbiRet, Susp},
};
use sbi_spec::hsm::suspend_type::NON_RETENTIVE;

const SUSPEND_TO_RAM: u32 = 0x0;

/// SBI System Suspend extension.
pub(crate) struct SbiSuspend;

impl Susp for SbiSuspend {
    fn system_suspend(&self, sleep_type: u32, resume_addr: usize, opaque: usize) -> SbiRet {
        if sleep_type != SUSPEND_TO_RAM {
            return SbiRet::invalid_param();
        }

        let prev_mode = Status::read().get::<MPP>();
        if prev_mode != Privilege::Supervisor && prev_mode != Privilege::User {
            return SbiRet::failed();
        }

        // SBI system suspend requires all other harts to be stopped.
        let Ok(current_hart) = HartId::current() else {
            return SbiRet::failed();
        };
        for hart in super::features::enabled_harts() {
            if hart != current_hart && hart::status(hart) != HartState::Stopped {
                return SbiRet::denied();
            }
        }

        // The HSM adapter validates the non-retentive resume entry address
        // before this hart suspends, returning `SBI_ERR_INVALID_ADDRESS` there.
        match crate::sbi::hsm() {
            Some(hsm) => hsm.hart_suspend(NON_RETENTIVE, resume_addr, opaque),
            None => SbiRet::not_supported(),
        }
    }
}
