//! Machine timer and IPI drivers backed by one or more CLINTs.

mod kind;
mod routing;
mod sifive;
mod thead;

use runtime::{
    Result,
    memory::{DeviceRegisterRange, MemoryRegistry},
};

pub(crate) use kind::ClintKind;
pub(crate) use routing::{RoutedIpi, RoutedTimer};

/// Hardware-local timer variants; only RoutedTimer implements Runtime's TimerDevice.
pub(super) enum ClintTimer {
    SiFive(sifive::SiFiveTimer),
    THead(thead::THeadTimer),
}

impl ClintTimer {
    fn set_compare_slot(&self, slot: usize, deadline: u64) {
        match self {
            Self::SiFive(timer) => timer.set_compare_slot(slot, deadline),
            Self::THead(timer) => timer.set_compare_slot(slot, deadline),
        }
    }

    fn read_time(&self) -> Option<u64> {
        match self {
            Self::SiFive(timer) => timer.read_time(),
            Self::THead(_) => None,
        }
    }
}

/// Hardware-local IPI variants; only RoutedIpi implements Runtime's IpiDevice.
pub(super) enum ClintIpi {
    SiFive(sifive::SiFiveIpi),
    THead(thead::THeadIpi),
}

impl ClintIpi {
    fn send_slot(&self, slot: usize) -> Result<()> {
        match self {
            Self::SiFive(ipi) => ipi.send_slot(slot),
            Self::THead(ipi) => ipi.send_slot(slot),
        }
    }

    fn clear_slot(&self, slot: usize) -> Result<()> {
        match self {
            Self::SiFive(ipi) => ipi.clear_slot(slot),
            Self::THead(ipi) => ipi.clear_slot(slot),
        }
    }
}

/// Bind one CLINT using physical slots (including disabled CPU references).
pub(super) fn bind(
    registers: DeviceRegisterRange,
    kind: ClintKind,
    memory: &mut MemoryRegistry,
    slot_count: usize,
) -> Result<(ClintTimer, ClintIpi)> {
    match kind {
        ClintKind::SiFive => {
            let (timer, ipi) = sifive::bind(registers, memory, slot_count)?;
            Ok((ClintTimer::SiFive(timer), ClintIpi::SiFive(ipi)))
        }
        ClintKind::THead => {
            let (timer, ipi) = thead::bind(registers, memory, slot_count)?;
            Ok((ClintTimer::THead(timer), ClintIpi::THead(ipi)))
        }
    }
}
