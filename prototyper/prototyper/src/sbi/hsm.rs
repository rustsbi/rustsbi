//! Hart-state protocol adapter.

use machine::{HartControl, HartError, HartStatus, NextStage};
use rustsbi::SbiRet;
use sbi_spec::hsm::{hart_state, suspend_type};

pub(super) struct Hsm {
    harts: HartControl,
}

impl Hsm {
    pub(super) fn new(harts: HartControl) -> Self {
        Self { harts }
    }
}

impl rustsbi::Hsm for Hsm {
    fn hart_start(&self, hart_id: usize, start_addr: usize, opaque: usize) -> SbiRet {
        let next_stage = match NextStage::supervisor(start_addr, opaque) {
            Ok(next_stage) => next_stage,
            Err(error) => return hart_error(error),
        };
        match self.harts.start(hart_id, next_stage) {
            Ok(()) => SbiRet::success(0),
            Err(error) => hart_error(error),
        }
    }

    fn hart_stop(&self) -> SbiRet {
        hart_error(self.harts.stop())
    }

    fn hart_get_status(&self, hart_id: usize) -> SbiRet {
        match self.harts.status(hart_id) {
            Ok(status) => SbiRet::success(status_value(status)),
            Err(error) => hart_error(error),
        }
    }

    fn hart_suspend(&self, kind: u32, resume_addr: usize, opaque: usize) -> SbiRet {
        match kind {
            suspend_type::RETENTIVE => match self.harts.suspend_retentive() {
                Ok(()) => SbiRet::success(0),
                Err(error) => hart_error(error),
            },
            suspend_type::NON_RETENTIVE => {
                let next_stage = match NextStage::supervisor(resume_addr, opaque) {
                    Ok(next_stage) => next_stage,
                    Err(error) => return hart_error(error),
                };
                hart_error(self.harts.suspend_non_retentive(next_stage))
            }
            _ => SbiRet::invalid_param(),
        }
    }
}

impl rustsbi::Susp for Hsm {
    fn system_suspend(&self, sleep_type: u32, resume_addr: usize, opaque: usize) -> SbiRet {
        const SUSPEND_TO_RAM: u32 = 0;

        if sleep_type != SUSPEND_TO_RAM {
            return SbiRet::invalid_param();
        }
        let next_stage = match NextStage::supervisor(resume_addr, opaque) {
            Ok(next_stage) => next_stage,
            Err(error) => return hart_error(error),
        };
        match self.harts.suspend_system(next_stage) {
            // A peer hart is still available, so the system-suspend entry
            // criteria are not satisfied. This meaning is specific to SUSP;
            // ordinary HSM calls retain the standard AlreadyAvailable mapping.
            HartError::AlreadyAvailable => SbiRet::denied(),
            error => hart_error(error),
        }
    }
}

fn status_value(status: HartStatus) -> usize {
    match status {
        HartStatus::Started => hart_state::STARTED,
        HartStatus::Stopped => hart_state::STOPPED,
        HartStatus::StartPending => hart_state::START_PENDING,
        HartStatus::StopPending => hart_state::STOP_PENDING,
        HartStatus::Suspended => hart_state::SUSPENDED,
        HartStatus::SuspendPending => hart_state::SUSPEND_PENDING,
        HartStatus::ResumePending => hart_state::RESUME_PENDING,
    }
}

fn hart_error(error: HartError) -> SbiRet {
    match error {
        HartError::InvalidHart => SbiRet::invalid_param(),
        HartError::InvalidAddress => SbiRet::invalid_address(),
        HartError::AlreadyAvailable => SbiRet::already_available(),
        HartError::NotSupported => SbiRet::not_supported(),
        HartError::Failed => SbiRet::failed(),
    }
}
