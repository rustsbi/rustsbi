#![forbid(unsafe_code)]

//! SBI HSM (Hart State Management) extension.

use riscv::register::mstatus::MPP;
use rustsbi::SbiRet;

use crate::riscv::csr::mie;
use crate::riscv::current_hartid;
use crate::sbi::hart_context::NextStage;

use super::trap::boot::boot;
use super::trap_stack::{RemoteHsmCell, hart_local, reset_hart};

/// Gets the local HSM cell for the current hart.
pub(crate) use super::trap_stack::local_hsm;

/// Gets a remote view of any hart's HSM cell.
pub(crate) use super::trap_stack::remote_hsm;

/// Returns a remote-capable view of the current hart's HSM cell.
pub(crate) fn hart_hsm() -> RemoteHsmCell<'static, NextStage> {
    hart_local(current_hartid()).hsm.remote()
}

/// Implementation of SBI HSM (Hart State Management) extension.
pub(crate) struct SbiHsm;

impl rustsbi::Hsm for SbiHsm {
    /// Starts execution on a stopped hart.
    fn hart_start(&self, hartid: usize, start_addr: usize, opaque: usize) -> SbiRet {
        let hart_enable = crate::platform::cpu_enabled().unwrap();
        let enabled = hart_enable.get(hartid).copied().unwrap_or(false);
        if !enabled {
            return SbiRet::invalid_param();
        }

        match remote_hsm(hartid) {
            Some(remote) => {
                if remote.start(NextStage {
                    start_addr,
                    opaque,
                    next_mode: MPP::Supervisor,
                }) {
                    crate::sbi::ipi().unwrap().set_msip(hartid);
                    SbiRet::success(0)
                } else {
                    SbiRet::already_available()
                }
            }
            None => SbiRet::invalid_param(),
        }
    }

    /// Stops execution on the current hart.
    #[inline]
    fn hart_stop(&self) -> SbiRet {
        local_hsm().stop();
        mie::disable_msoft();
        riscv::asm::wfi();
        SbiRet::success(0)
    }

    /// Gets the current state of a hart.
    #[inline]
    fn hart_get_status(&self, hartid: usize) -> SbiRet {
        let hart_enable = crate::platform::cpu_enabled().unwrap();
        let enabled = hart_enable.get(hartid).copied().unwrap_or(false);
        if !enabled {
            return SbiRet::invalid_param();
        }

        match remote_hsm(hartid) {
            Some(remote) => SbiRet::success(remote.get_status()),
            None => SbiRet::invalid_param(),
        }
    }

    /// Suspends execution on the current hart.
    fn hart_suspend(&self, suspend_type: u32, resume_addr: usize, opaque: usize) -> SbiRet {
        use rustsbi::spec::hsm::suspend_type::{NON_RETENTIVE, RETENTIVE};

        if !matches!(suspend_type, NON_RETENTIVE | RETENTIVE) {
            return SbiRet::invalid_param();
        }

        crate::sbi::trap::handler::msoft_ipi_handler();
        crate::sbi::ipi().unwrap().clear_msip(current_hartid());
        mie::enable_msoft();
        local_hsm().suspend();
        riscv::asm::wfi();
        crate::sbi::trap::handler::msoft_ipi_handler();

        match suspend_type {
            RETENTIVE => {
                local_hsm().resume();
                return SbiRet::success(0);
            }
            NON_RETENTIVE => return self.hart_resume(current_hartid(), resume_addr, opaque),
            _ => return SbiRet::invalid_param(),
        }
    }
}

impl SbiHsm {
    // non retentive resume
    fn hart_resume(&self, hartid: usize, resume_addr: usize, opaque: usize) -> SbiRet {
        match remote_hsm(hartid) {
            Some(remote) => {
                if remote.resume(NextStage {
                    start_addr: resume_addr,
                    opaque,
                    next_mode: MPP::Supervisor,
                }) {
                    // reset the hart local context to prevent the hart context from being polluted
                    reset_hart(hartid);
                    // boot resume hart from resume addr
                    boot();
                } else {
                    SbiRet::failed()
                }
            }
            None => SbiRet::failed(),
        }
    }
}
