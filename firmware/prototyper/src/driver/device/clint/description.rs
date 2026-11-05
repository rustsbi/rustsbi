//! CLINT timer and software-interrupt descriptions.

use super::{TimerDesc, ipi};

/// The timer and software IPI capabilities described by one CLINT node.
pub(crate) struct Desc {
    pub(crate) ipi: ipi::Desc,
    pub(crate) timer: TimerDesc,
}
