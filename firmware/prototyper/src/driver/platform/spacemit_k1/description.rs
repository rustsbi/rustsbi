//! SpacemiT K1 platform description.

use runtime::SpacemitK1Registers;

/// K1 system registers collected from the platform description.
pub(crate) struct Desc {
    pub(super) registers: SpacemitK1Registers,
}

impl Desc {
    pub(crate) const fn new(registers: SpacemitK1Registers) -> Self {
        Self { registers }
    }
}
