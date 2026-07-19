//! Inter-processor interrupt protocol adapter.

use machine::{HartTargets, Ipi as MachineIpi, IpiError};
use rustsbi::{HartMask, SbiRet};

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
        match self.ipi.send(targets(hart_mask)) {
            Ok(()) => SbiRet::success(0),
            Err(IpiError::InvalidHart) => SbiRet::invalid_param(),
            Err(IpiError::Failed) => SbiRet::failed(),
        }
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
