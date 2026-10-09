//! SBI Hart State Management adapter.
//!
//! The SBI ABI and enabled-hart policy live here. Runtime owns hart-state
//! transitions and privileged entry.
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

    /// Validates an enabled SBI hart ID against platform privilege policy.
    fn hart_id(&self, raw: usize) -> Result<HartId, SbiRet> {
        let hart = HartId::from_raw(raw).map_err(|_| SbiRet::invalid_param())?;
        if !(self.hardware_wakeup || crate::platform::hart_privilege_checked(raw)) {
            return Err(SbiRet::invalid_param());
        }
        Ok(hart)
    }

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
        if let Err(error) = crate::sbi::hart_local::reset_current() {
            warn!("PMU reset before non-retentive resume failed: {error:?}");
            drop(ticket);
            return resume_original_context_after_failure();
        }
        match ticket.commit() {
            Ok(()) => SbiRet::success(0),
            Err(error) => {
                warn!("Non-retentive resume failed: {error:?}");
                resume_original_context_after_failure()
            }
        }
    }
}

/// Restores the hart's running state before returning to its retained supervisor.
/// The failed or dropped resume ticket has already restored Suspended.
fn resume_original_context_after_failure() -> SbiRet {
    if let Err(error) = hart::resume_current_retentive() {
        error!("Cannot restore hart state after failed non-retentive resume: {error:?}");
        crate::fail::stop();
    }
    SbiRet::failed()
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

    /// Returns the current state of a hart.
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
