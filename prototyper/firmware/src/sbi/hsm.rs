//! Hart-state protocol adapter.

use machine::{HartControl, HartError, NextStage};
use rustsbi::SbiRet;
use sbi_spec::binary::Error;
use sbi_spec::hsm::suspend_type;

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
            Err(error) => return hart_error(error).into(),
        };
        match self.harts.start(hart_id, next_stage) {
            Ok(()) => SbiRet::success(0),
            Err(error) => hart_error(error).into(),
        }
    }

    fn hart_stop(&self) -> SbiRet {
        hart_error(self.harts.stop()).into()
    }

    fn hart_get_status(&self, hart_id: usize) -> SbiRet {
        match self.harts.status(hart_id) {
            Ok(status) => SbiRet::success(status as usize),
            Err(error) => hart_error(error).into(),
        }
    }

    fn hart_suspend(&self, kind: u32, resume_addr: usize, opaque: usize) -> SbiRet {
        match kind {
            suspend_type::RETENTIVE => match self.harts.suspend_retentive() {
                Ok(()) => SbiRet::success(0),
                Err(error) => hart_error(error).into(),
            },
            suspend_type::NON_RETENTIVE => {
                let next_stage = match NextStage::supervisor(resume_addr, opaque) {
                    Ok(next_stage) => next_stage,
                    Err(error) => return hart_error(error).into(),
                };
                hart_error(self.harts.suspend_non_retentive(next_stage)).into()
            }
            _ => Error::InvalidParam.into(),
        }
    }
}

impl rustsbi::Susp for Hsm {
    fn system_suspend(&self, sleep_type: u32, resume_addr: usize, opaque: usize) -> SbiRet {
        const SUSPEND_TO_RAM: u32 = 0;

        if sleep_type != SUSPEND_TO_RAM {
            return Error::InvalidParam.into();
        }
        let next_stage = match NextStage::supervisor(resume_addr, opaque) {
            Ok(next_stage) => next_stage,
            Err(error) => return hart_error(error).into(),
        };
        match self.harts.suspend_system(next_stage) {
            // A peer hart is still available, so the system-suspend entry
            // criteria are not satisfied. This meaning is specific to SUSP;
            // ordinary HSM calls retain the standard AlreadyAvailable mapping.
            HartError::AlreadyAvailable => Error::Denied.into(),
            error => hart_error(error).into(),
        }
    }
}

fn hart_error(error: HartError) -> Error {
    match error {
        HartError::InvalidHart => Error::InvalidParam,
        HartError::InvalidAddress => Error::InvalidAddress,
        HartError::AlreadyAvailable => Error::AlreadyAvailable,
        HartError::NotSupported => Error::NotSupported,
        HartError::Failed => Error::Failed,
    }
}
