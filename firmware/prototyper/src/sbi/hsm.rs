//! SBI Hart State Management adapter.
//!
//! The SBI ABI and platform enabled-hart policy live here.  Runtime owns the
//! protocol-free hart state machine and all privileged transitions.
//!
//! # References
//!
//! - Specification: [RISC-V SBI HSM extension](https://docs.riscv.org/reference/sbi/v3.0/ext-hsm.html) —
//!   hart states and start, stop, and suspend transitions.

#![forbid(unsafe_code)]

use runtime::boot::NextStage;
use runtime::hart::{self, HartId, HartState};
use runtime::rustsbi::{Hsm, SbiRet};
use sbi_spec::hsm::{hart_state, suspend_type};

/// SBI HSM extension composed from platform policy and Runtime hart
/// mechanism.
pub(crate) struct SbiHsm {
    hardware_wakeup: bool,
}

impl SbiHsm {
    /// Records whether the platform can release a hart from hardware reset.
    pub(crate) fn new(hardware_wakeup: bool) -> Self {
        Self { hardware_wakeup }
    }

    /// Validates an SBI hart ID against Runtime capacity and platform policy.
    fn hart_id(&self, raw: usize) -> Result<HartId, SbiRet> {
        let hart = HartId::from_raw(raw).map_err(|_| SbiRet::invalid_param())?;
        if crate::platform::board_info().harts.enabled.get(raw) != Some(&true)
            || !(self.hardware_wakeup || crate::platform::hart_privilege_checked(raw))
        {
            return Err(SbiRet::invalid_param());
        }
        Ok(hart)
    }

    /// Implements the HSM suspend/resume transition described by the SBI
    /// HSM extension.
    fn suspend(&self, suspend_type: u32, resume_addr: usize, opaque: usize) -> SbiRet {
        if !matches!(
            suspend_type,
            suspend_type::RETENTIVE | suspend_type::NON_RETENTIVE
        ) {
            return SbiRet::invalid_param();
        }
        if hart::suspend_current().is_err() {
            return SbiRet::failed();
        }

        if suspend_type == suspend_type::RETENTIVE {
            return match hart::resume_current_retentive() {
                Ok(()) => SbiRet::success(0),
                Err(_) => SbiRet::failed(),
            };
        }

        let stage = NextStage {
            start_addr: resume_addr,
            opaque,
            next_mode: riscv::register::mstatus::MPP::Supervisor,
        };
        let ticket = match hart::begin_nonretentive_resume(stage) {
            Ok(ticket) => ticket,
            Err(_) => return SbiRet::failed(),
        };

        // The ticket reserves the current hart before policy state is reset;
        // a failed wake drops the reservation back to Suspended.
        crate::sbi::hart_local::reset_current();
        match ticket.commit() {
            Ok(()) => SbiRet::success(0),
            Err(_) => SbiRet::failed(),
        }
    }
}

impl Hsm for SbiHsm {
    /// Starts execution on a stopped hart.
    fn hart_start(&self, hartid: usize, start_addr: usize, opaque: usize) -> SbiRet {
        let hart = match self.hart_id(hartid) {
            Ok(hart) => hart,
            Err(error) => return error,
        };
        let stage = NextStage {
            start_addr,
            opaque,
            next_mode: riscv::register::mstatus::MPP::Supervisor,
        };
        match hart::start(hart, stage) {
            Ok(hart::StartOutcome::Accepted) => SbiRet::success(0),
            Ok(hart::StartOutcome::AlreadyRunning) => SbiRet::already_available(),
            Err(hart::StartError::WakeFailed) => SbiRet::failed(),
        }
    }

    /// Stops execution on the current hart.
    #[inline]
    fn hart_stop(&self) -> SbiRet {
        match hart::stop_current() {
            Ok(never) => match never {},
            Err(_) => SbiRet::failed(),
        }
    }

    /// Gets the current state of a hart.
    #[inline]
    fn hart_get_status(&self, hartid: usize) -> SbiRet {
        let hart = match self.hart_id(hartid) {
            Ok(hart) => hart,
            Err(error) => return error,
        };
        let state = match hart::status(hart) {
            HartState::Started => hart_state::STARTED,
            HartState::Stopped => hart_state::STOPPED,
            HartState::StartPending => hart_state::START_PENDING,
            HartState::Suspended => hart_state::SUSPENDED,
            HartState::ResumePending => hart_state::RESUME_PENDING,
        };
        SbiRet::success(state)
    }

    /// Suspends and resumes the current hart through Runtime's mechanism.
    #[inline]
    fn hart_suspend(&self, suspend_type: u32, resume_addr: usize, opaque: usize) -> SbiRet {
        self.suspend(suspend_type, resume_addr, opaque)
    }
}
