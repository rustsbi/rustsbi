//! Inter-processor interrupt protocol adapter.

use machine::{HartTargets, Ipi as MachineIpi, IpiError};
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
                .send(targets(hart_mask))
                .map(|()| 0)
                .map_err(|error| match error {
                    IpiError::InvalidHart => Error::InvalidParam,
                    IpiError::Failed => Error::Failed,
                }),
        )
    }
}

pub(super) fn targets(mask: HartMask) -> HartTargets {
    let (bits, base) = mask.into_inner();
    if base == usize::MAX {
        HartTargets::all_available()
    } else {
        HartTargets::selected(bits, base)
    }
}
