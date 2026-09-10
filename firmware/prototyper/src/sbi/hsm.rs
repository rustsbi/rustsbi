//! Hart state management.
//!
//! # References
//!
//! - Specification: [RISC-V SBI HSM extension](https://docs.riscv.org/reference/sbi/v3.0/ext-hsm.html) —
//!   hart states and state transitions.

#![forbid(unsafe_code)]

use alloc::boxed::Box;
use riscv::register::mstatus::MPP;
use rustsbi::SbiRet;
use spin::Mutex;

use crate::driver::{HartWake, IpiError};
use crate::platform;

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

/// SBI HSM extension service.
pub(crate) struct SbiHsm {
    wakeup: Option<Mutex<Box<dyn HartWake>>>,
}

impl rustsbi::Hsm for SbiHsm {
    /// Starts execution on a stopped hart.
    fn hart_start(&self, hartid: usize, start_addr: usize, opaque: usize) -> SbiRet {
        if !self.hart_available(hartid) {
            return SbiRet::invalid_param();
        }

        match remote_hsm(hartid) {
            Some(remote) => {
                match remote.start_with(
                    NextStage {
                        start_addr,
                        opaque,
                        next_mode: MPP::Supervisor,
                    },
                    || self.wake_hart(hartid),
                ) {
                    Ok(true) => SbiRet::success(0),
                    Ok(false) => SbiRet::already_available(),
                    Err(crate::driver::IpiError::Failed) => SbiRet::failed(),
                }
            }
            None => SbiRet::invalid_param(),
        }
    }

    /// Stops execution on the current hart.
    #[inline]
    fn hart_stop(&self) -> SbiRet {
        if crate::sbi::ipi()
            .unwrap()
            .clear_ipi(current_hartid())
            .is_err()
        {
            return SbiRet::failed();
        }
        mie::enable_msoft();
        local_hsm().stop();
        // A stopped hart must remain in M-mode, including after spurious
        // WFI wakeups. Keep MSIE enabled so a later hart_start can wake it.
        while hart_hsm().get_status() == rustsbi::spec::hsm::hart_state::STOPPED {
            riscv::asm::wfi();
        }
        boot()
    }

    /// Gets the current state of a hart.
    #[inline]
    fn hart_get_status(&self, hartid: usize) -> SbiRet {
        if !self.hart_available(hartid) {
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
        if crate::sbi::ipi()
            .unwrap()
            .clear_ipi(current_hartid())
            .is_err()
        {
            return SbiRet::failed();
        }
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
    pub(crate) fn new(wakeup: Option<Box<dyn HartWake>>) -> Self {
        Self {
            wakeup: wakeup.map(Mutex::new),
        }
    }

    fn hart_available(&self, hart_id: usize) -> bool {
        // A hardware-startable hart can be STOPPED before its first entry.
        platform::board_info().enabled_harts.get(hart_id) == Some(&true)
            && (self.wakeup.is_some() || platform::hart_privilege_checked(hart_id))
    }

    fn wake_hart(&self, hart_id: usize) -> Result<(), IpiError> {
        if let Some(wakeup) = &self.wakeup
            && wakeup.lock().wake(hart_id).map_err(|_| IpiError::Failed)?
        {
            return Ok(());
        }
        crate::sbi::ipi().unwrap().send_ipi(hart_id)
    }

    /// Non-retentive resume: restarts this hart at `resume_addr` from a
    /// clean context.
    fn hart_resume(&self, hartid: usize, resume_addr: usize, opaque: usize) -> SbiRet {
        match remote_hsm(hartid) {
            Some(remote) => {
                if remote.resume(NextStage {
                    start_addr: resume_addr,
                    opaque,
                    next_mode: MPP::Supervisor,
                }) {
                    // Reset hart-local context so the resumed hart starts
                    // from a clean state.
                    reset_hart(hartid);
                    boot();
                } else {
                    SbiRet::failed()
                }
            }
            None => SbiRet::failed(),
        }
    }
}
