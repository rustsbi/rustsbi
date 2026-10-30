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

use alloc::boxed::Box;
use runtime::{
    Result,
    boot::NextStage,
    hart::{Hart, HartId, HartState, HartWake, StartOutcome},
    rustsbi::{Hsm, SbiRet},
};
use sbi_spec::hsm::{hart_state, suspend_type};

/// SBI HSM extension composed from platform policy and Runtime hart mechanism.
pub(crate) struct SbiHsm {
    start_support: HartStartSupport,
    wake: Option<Box<dyn HartWake>>,
}

/// Which harts the platform can bring into firmware execution.
#[derive(Clone, Copy, Eq, PartialEq)]
pub enum HartStartSupport {
    /// Only harts that have already initialized their firmware context.
    InitializedHarts,
    /// A platform wakeup device can also release harts held in hardware reset.
    IncludesResetHarts,
}

impl SbiHsm {
    /// Records the platform's hart-start capability.
    pub(crate) fn new(start_support: HartStartSupport, wake: Option<Box<dyn HartWake>>) -> Self {
        Self {
            start_support,
            wake,
        }
    }

    /// Checks whether the platform can wake this target for a start request.
    fn check_start_support(&self, hart: HartId) -> Result<(), SbiRet> {
        match self.start_support {
            HartStartSupport::IncludesResetHarts => Ok(()),
            HartStartSupport::InitializedHarts => {
                if crate::platform::hart_privilege_checked(hart.as_usize()) {
                    Ok(())
                } else {
                    Err(SbiRet::invalid_param())
                }
            }
        }
    }
}

/// Finds an enabled hart without requiring it to have entered firmware yet.
fn find_hart(hart_id: usize) -> Result<&'static Hart, SbiRet> {
    let id = HartId::from_raw(hart_id).map_err(|_| SbiRet::invalid_param())?;
    Hart::get(id).map_err(Into::into)
}

impl Hsm for SbiHsm {
    /// Starts execution on a stopped hart.
    fn hart_start(&self, hartid: usize, start_addr: usize, opaque: usize) -> SbiRet {
        let hart = match find_hart(hartid) {
            Ok(hart) => hart,
            Err(error) => return error,
        };
        if let Err(error) = self.check_start_support(hart.id()) {
            return error;
        }
        let stage = NextStage::supervisor(start_addr, opaque);
        match hart.start(stage, self.wake.as_deref()) {
            Ok(StartOutcome::Accepted) => SbiRet::success(0),
            Ok(StartOutcome::NotStopped(HartState::Started)) => SbiRet::already_available(),
            Ok(StartOutcome::NotStopped(_)) | Err(_) => SbiRet::failed(),
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
        let hart = match find_hart(hartid) {
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
