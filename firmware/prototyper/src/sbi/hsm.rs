//! SBI Hart State Management adapter.
//!
//! The SBI ABI and platform enabled-hart policy live here.
//! Runtime owns the protocol-free hart state machine and all privileged
//! transitions.
//!
//! # References
//!
//! - Specification: [RISC-V SBI HSM extension](https://docs.riscv.org/reference/sbi/v3.0/ext-hsm.html) —
//!   hart states and start, stop, and suspend transitions.

#![forbid(unsafe_code)]

use runtime::{
    Result,
    boot::NextStage,
    hart::{Hart, HartId, HartState, StartOutcome},
    rustsbi::{Hsm, SbiRet},
};
use sbi_spec::hsm::{hart_state, suspend_type};

/// SBI HSM extension composed from platform policy and Runtime hart mechanism.
pub(crate) struct SbiHsm {
    hardware_wakeup: bool,
}

impl SbiHsm {
    /// Records whether the platform can release a hart from hardware reset.
    pub(crate) fn new(hardware_wakeup: bool) -> Self {
        Self { hardware_wakeup }
    }

    /// Validates an enabled SBI hart ID against platform privilege policy.
    fn hart(&self, raw: usize) -> Result<&'static Hart, SbiRet> {
        let hart = HartId::from_raw(raw).map_err(|_| SbiRet::invalid_param())?;
        if !(self.hardware_wakeup || crate::platform::hart_privilege_checked(raw)) {
            return Err(SbiRet::invalid_param());
        }
        Hart::get(hart).map_err(Into::into)
    }

    /// Implements the HSM suspend/resume transition described by the SBI HSM
    /// extension.
    fn suspend(&self, suspend_type: u32, resume_addr: usize, opaque: usize) -> SbiRet {
        if !matches!(
            suspend_type,
            suspend_type::RETENTIVE | suspend_type::NON_RETENTIVE
        ) {
            return SbiRet::invalid_param();
        }
        let hart = match Hart::current() {
            Ok(hart) => hart,
            Err(error) => return error.into(),
        };
        if hart.suspend().is_err() {
            return SbiRet::failed();
        }

        if suspend_type == suspend_type::RETENTIVE {
            return match hart.resume_retentive() {
                Ok(()) => SbiRet::success(0),
                Err(_) => SbiRet::failed(),
            };
        }

        let stage = NextStage::supervisor(resume_addr, opaque);
        match hart.resume_nonretentive(stage) {
            Ok(()) => SbiRet::success(0),
            Err(_) => SbiRet::failed(),
        }
    }
}

impl Hsm for SbiHsm {
    /// Starts execution on a stopped hart.
    fn hart_start(&self, hartid: usize, start_addr: usize, opaque: usize) -> SbiRet {
        let hart = match self.hart(hartid) {
            Ok(hart) => hart,
            Err(error) => return error,
        };
        let stage = NextStage::supervisor(start_addr, opaque);
        match hart.start(stage) {
            Ok(StartOutcome::Accepted) => SbiRet::success(0),
            Ok(StartOutcome::AlreadyRunning) => SbiRet::already_available(),
            Err(_) => SbiRet::failed(),
        }
    }

    /// Stops execution on the current hart.
    #[inline]
    fn hart_stop(&self) -> SbiRet {
        match Hart::current().and_then(Hart::stop) {
            Ok(()) => SbiRet::success(0),
            Err(_) => SbiRet::failed(),
        }
    }

    /// Gets the current state of a hart.
    #[inline]
    fn hart_get_status(&self, hartid: usize) -> SbiRet {
        let hart = match self.hart(hartid) {
            Ok(hart) => hart,
            Err(error) => return error,
        };
        let state = match hart.status() {
            HartState::Started => hart_state::STARTED,
            HartState::Stopped => hart_state::STOPPED,
            HartState::StopPending => hart_state::STOP_PENDING,
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
