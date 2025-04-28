use riscv::register::mstatus;
use rustsbi::{Hsm, SbiRet};
use sbi_spec::hsm::{hart_state::STOPPED, suspend_type::NON_RETENTIVE};

use crate::{platform::PLATFORM, riscv::current_hartid};

use super::hsm::remote_hsm;

const SUSPEND_TO_RAM: u32 = 0x0;

/// Implementation of SBI System Suspend Extension extension.
pub(crate) struct SbiSuspend;

impl rustsbi::Susp for SbiSuspend {
    fn system_suspend(&self, sleep_type: u32, resume_addr: usize, opaque: usize) -> SbiRet {
        if sleep_type != SUSPEND_TO_RAM {
            return SbiRet::invalid_param();
        }

        let prev_mode = mstatus::read().mpp();
        if prev_mode != mstatus::MPP::Supervisor || prev_mode != mstatus::MPP::User {
            return SbiRet::failed();
        }

        // Check if all harts except the current hart are stopped
        let hart_enable_map = if let Some(hart_enable_map) = unsafe { PLATFORM.info.cpu_enabled } {
            hart_enable_map
        } else {
            return SbiRet::failed();
        };
        for (hartid, hart_enable) in hart_enable_map.iter().enumerate() {
            if *hart_enable && hartid != current_hartid() {
                match remote_hsm(hartid) {
                    Some(remote) => {
                        if remote.get_status() != STOPPED {
                            return SbiRet::denied();
                        }
                    }
                    None => return SbiRet::failed(),
                }
            }
        }

        // TODO: The validity of `resume_addr` should be checked.
        // If it is invalid, `SBI_ERR_INVALID_ADDRESS` should be returned.

        if let Some(hsm) = unsafe { &PLATFORM.sbi.hsm } {
            hsm.hart_suspend(NON_RETENTIVE, resume_addr, opaque);
        } else {
            return SbiRet::not_supported();
        }

        unreachable!();
    }
}
