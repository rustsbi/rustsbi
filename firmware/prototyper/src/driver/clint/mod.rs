//! Machine timer and IPI drivers backed by a CLINT.

mod kind;
mod sifive;
mod thead;

use alloc::boxed::Box;

use crate::driver::{InterruptDevices, IpiDevice, TimerDevice};
use crate::platform::BoardInfo;
use crate::platform::mmio::Mmio;

pub(crate) use kind::ClintKind;

/// Constructs the CLINT timer and IPI devices selected during discovery.
pub(super) fn from_board(board: &BoardInfo) -> Option<InterruptDevices> {
    let (base, kind) = board.ipi.as_ref()?;
    match *kind {
        ClintKind::Sifive => devices(board, *base, sifive::SPAN, sifive::SifiveClint::new),
        ClintKind::THead => devices(board, *base, thead::SPAN, thead::THeadClint::new),
    }
}

fn devices<D>(
    board: &BoardInfo,
    base: usize,
    span: usize,
    constructor: fn(Mmio) -> D,
) -> Option<InterruptDevices>
where
    D: IpiDevice + TimerDevice + 'static,
{
    let Some(mmio) = Mmio::within(board, base, span) else {
        warn!(
            "CLINT: FDT reg window at {:#x} is smaller than the required {:#x}-byte span, skipping",
            base, span
        );
        return None;
    };
    Some(InterruptDevices {
        timer: Box::new(constructor(mmio)),
        ipi: Box::new(constructor(mmio)),
    })
}
