//! Inter-processor interrupt protocol adapter.

use machine::{Ipi as MachineIpi, IpiError};
use rustsbi::{HartMask, SbiRet};
use sbi_spec::binary::Error;

use super::response;
pub(super) struct Ipi {
    ipi: MachineIpi,
}

impl Ipi {
    pub(super) fn new(ipi: MachineIpi) -> Self {
        Self { ipi }
    }
}

impl rustsbi::Ipi for Ipi {
    fn send_ipi(&self, hart_mask: HartMask) -> SbiRet {
        response(
            self.ipi
                .send(hart_mask)
                .map(|()| 0)
                .map_err(|error| match error {
                    IpiError::InvalidHart => Error::InvalidParam,
                    IpiError::Failed => Error::Failed,
                }),
        )
    }
}
