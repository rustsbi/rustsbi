//! SpacemiT K1 platform description.

use runtime::soc::spacemit::k1::SpacemitK1Soc;

/// K1 system registers collected from the platform description.
pub(crate) struct Desc {
    pub(super) soc: SpacemitK1Soc,
}
